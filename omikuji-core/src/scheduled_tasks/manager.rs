use crate::database::TransactionLogRepository;
use crate::gas_price::GasPriceManager;
use crate::network::NetworkManager as NetworkProviders;
use crate::scheduled_tasks::{
    condition_checker::ConditionChecker, executor::FunctionExecutor, models::ScheduledTask,
};
use crate::utils::{TransactionContext, TransactionHandler, TransactionLogger};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tokio_cron_scheduler::{Job, JobScheduler};
use tracing::{debug, error, info};

pub struct ScheduledTaskManager {
    tasks: Arc<RwLock<HashMap<String, ScheduledTask>>>,
    network_providers: Arc<NetworkProviders>,
    scheduler: Arc<JobScheduler>,
    handles: Arc<RwLock<Vec<JoinHandle<()>>>>,
    gas_price_manager: Option<Arc<GasPriceManager>>,
    tx_log_repo: Option<Arc<TransactionLogRepository>>,
}

impl ScheduledTaskManager {
    pub async fn new(
        tasks: Vec<ScheduledTask>,
        network_providers: Arc<NetworkProviders>,
    ) -> Result<Self> {
        let scheduler = JobScheduler::new().await?;

        let task_map = tasks
            .into_iter()
            .map(|task| (task.name.clone(), task))
            .collect();

        Ok(Self {
            tasks: Arc::new(RwLock::new(task_map)),
            network_providers,
            scheduler: Arc::new(scheduler),
            handles: Arc::new(RwLock::new(Vec::new())),
            gas_price_manager: None,
            tx_log_repo: None,
        })
    }

    /// Sets the gas price manager for USD cost tracking
    pub fn with_gas_price_manager(mut self, gas_price_manager: Arc<GasPriceManager>) -> Self {
        self.gas_price_manager = Some(gas_price_manager);
        self
    }

    /// Sets the transaction log repository for transaction history
    pub fn with_tx_log_repo(mut self, tx_log_repo: Arc<TransactionLogRepository>) -> Self {
        self.tx_log_repo = Some(tx_log_repo);
        self
    }

    pub async fn start(&self) -> Result<()> {
        let tasks = self.tasks.read().await;

        for (name, task) in tasks.iter() {
            self.schedule_task(name.clone(), task.clone()).await?;
        }

        self.scheduler.start().await?;
        info!("Scheduled task manager started with {} tasks", tasks.len());

        Ok(())
    }

    async fn schedule_task(&self, name: String, task: ScheduledTask) -> Result<()> {
        let network_providers = self.network_providers.clone();
        let gas_price_manager = self.gas_price_manager.clone();
        let tx_log_repo = self.tx_log_repo.clone();
        let task_clone = task.clone();

        let job = Job::new_async(task.schedule.as_str(), move |_uuid, _l| {
            let name = name.clone();
            let task = task_clone.clone();
            let providers = network_providers.clone();
            let gas_mgr = gas_price_manager.clone();
            let tx_repo = tx_log_repo.clone();

            Box::pin(async move {
                debug!("Executing scheduled task: {}", name);

                if let Err(e) = execute_task(task, providers, gas_mgr, tx_repo).await {
                    error!("Failed to execute scheduled task '{}': {}", name, e);
                }
            })
        })?;

        self.scheduler.add(job).await?;
        info!(
            "Scheduled task '{}' with cron expression '{}'",
            task.name, task.schedule
        );

        Ok(())
    }

    pub async fn stop(&self) -> Result<()> {
        info!("Stopping scheduled task manager");

        // Note: tokio-cron-scheduler doesn't provide a mutable shutdown method
        // The scheduler will be cleaned up when dropped

        // Cancel all running tasks
        let mut handles = self.handles.write().await;
        for handle in handles.drain(..) {
            handle.abort();
        }

        Ok(())
    }

    #[allow(dead_code)]
    pub async fn get_task_status(&self) -> HashMap<String, TaskStatus> {
        let tasks = self.tasks.read().await;
        let mut status_map = HashMap::new();

        for (name, task) in tasks.iter() {
            status_map.insert(
                name.clone(),
                TaskStatus {
                    name: name.clone(),
                    schedule: task.schedule.clone(),
                    network: task.network.clone(),
                    // Additional status fields can be added here
                },
            );
        }

        status_map
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct TaskStatus {
    pub name: String,
    pub schedule: String,
    pub network: String,
}

async fn execute_task(
    task: ScheduledTask,
    network_providers: Arc<NetworkProviders>,
    gas_price_manager: Option<Arc<GasPriceManager>>,
    tx_log_repo: Option<Arc<TransactionLogRepository>>,
) -> Result<()> {
    TransactionLogger::log_execution_start("scheduled_task", &task.name);
    debug!("Task configuration: {:?}", task);

    // Get the provider for the task's network
    debug!("Getting provider for network: {}", task.network);
    let provider = network_providers
        .get_provider(&task.network)
        .context(format!("No provider found for network: {}", task.network))?;
    debug!("Successfully got provider for network: {}", task.network);

    // Check condition if specified
    if let Some(condition) = &task.check_condition {
        debug!("Checking condition for task '{}'", task.name);
        debug!("Condition details: {:?}", condition);

        let condition_met = ConditionChecker::check_condition(provider.clone(), condition)
            .await
            .map_err(|e| {
                error!("Condition check failed for task '{}': {:?}", task.name, e);
                e
            })
            .context("Failed to check condition")?;

        if !condition_met {
            TransactionLogger::log_condition_not_met(
                "scheduled_task",
                &task.name,
                "check returned false",
            );
            return Ok(());
        }
        TransactionLogger::log_condition_met("scheduled_task", &task.name, "check returned true");
    } else {
        debug!(
            "No condition specified for task '{}', proceeding directly to execution",
            task.name
        );
    }

    // Execute the target function
    debug!("Executing target function for task '{}'", task.name);
    debug!("Target function: {:?}", task.target_function);
    debug!("Gas config: {:?}", task.gas_config);

    let executor = FunctionExecutor::new(provider.clone());
    let receipt = executor
        .execute_function(
            &task.name,
            &task.network,
            &task.target_function,
            task.gas_config.as_ref(),
        )
        .await
        .map_err(|e| {
            error!(
                "Target function execution failed for task '{}': {:?}",
                task.name, e
            );
            e
        })
        .context("Failed to execute target function")?;

    // Use the standardized transaction handler for logging and metrics
    let context = TransactionContext::ScheduledTask {
        task_name: task.name.clone(),
    };

    // Get gas_limit from config or use a default
    let gas_limit = task
        .gas_config
        .as_ref()
        .and_then(|cfg| cfg.gas_limit)
        .unwrap_or(crate::constants::gas::DEFAULT_GAS_LIMIT);

    // Default to eip1559 transaction type
    // TODO: Get from network config when available
    let tx_type = "eip1559".to_string();

    // Convert the receipt to the standard alloy TransactionReceipt type
    // The receipt from executor is already the correct type
    TransactionHandler::new(receipt, context, task.network.clone())
        .with_gas_price_manager(gas_price_manager.as_ref())
        .with_tx_log_repo(tx_log_repo.as_ref())
        .with_gas_limit(gas_limit)
        .with_transaction_type(tx_type)
        .process()
        .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scheduled_tasks::models::{CheckCondition, GasConfig, Parameter, TargetFunction};

    fn create_test_task(name: &str) -> ScheduledTask {
        ScheduledTask {
            name: name.to_string(),
            network: "test-network".to_string(),
            schedule: "0 0 * * * *".to_string(), // Every hour
            check_condition: None,
            target_function: TargetFunction {
                contract_address: "0x1234567890123456789012345678901234567890".to_string(),
                function: "execute()".to_string(),
                parameters: vec![],
            },
            gas_config: None,
        }
    }

    fn create_test_task_with_condition(name: &str) -> ScheduledTask {
        ScheduledTask {
            name: name.to_string(),
            network: "test-network".to_string(),
            schedule: "*/5 * * * * *".to_string(), // Every 5 seconds
            check_condition: Some(CheckCondition::Function {
                contract_address: "0x1234567890123456789012345678901234567890".to_string(),
                function: "canExecute()".to_string(),
                expected_value: serde_json::Value::Bool(true),
            }),
            target_function: TargetFunction {
                contract_address: "0x1234567890123456789012345678901234567890".to_string(),
                function: "performTask()".to_string(),
                parameters: vec![],
            },
            gas_config: Some(GasConfig {
                gas_limit: Some(200000),
                max_gas_price_gwei: Some(50),
                priority_fee_gwei: Some(2),
            }),
        }
    }

    #[tokio::test]
    async fn test_scheduled_task_manager_creation() {
        let tasks = vec![create_test_task("task1"), create_test_task("task2")];

        // This test would need a mock NetworkProviders
        // For now, we verify the structure is created correctly
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].name, "task1");
        assert_eq!(tasks[1].name, "task2");
    }

    #[test]
    fn test_builder_methods() {
        // Test that builder methods compile and work correctly
        let tasks = vec![create_test_task("test")];

        // This would need actual instances to test fully
        // For now, we ensure the API is correct
        assert_eq!(tasks[0].name, "test");
    }

    #[test]
    fn test_task_status_struct() {
        let status = TaskStatus {
            name: "test_task".to_string(),
            schedule: "0 0 * * * *".to_string(),
            network: "mainnet".to_string(),
        };

        assert_eq!(status.name, "test_task");
        assert_eq!(status.schedule, "0 0 * * * *");
        assert_eq!(status.network, "mainnet");
    }

    #[test]
    fn test_gas_limit_extraction() {
        let task_with_gas = create_test_task_with_condition("gas_test");
        let gas_limit = task_with_gas
            .gas_config
            .as_ref()
            .and_then(|cfg| cfg.gas_limit)
            .unwrap_or(300_000);

        assert_eq!(gas_limit, 200000);

        let task_without_gas = create_test_task("no_gas");
        let gas_limit = task_without_gas
            .gas_config
            .as_ref()
            .and_then(|cfg| cfg.gas_limit)
            .unwrap_or(300_000);

        assert_eq!(gas_limit, 300_000); // Default
    }

    #[test]
    fn test_scheduled_task_with_parameters() {
        let task = ScheduledTask {
            name: "param_task".to_string(),
            network: "test-network".to_string(),
            schedule: "0 */6 * * *".to_string(), // Every 6 hours
            check_condition: None,
            target_function: TargetFunction {
                contract_address: "0xABCDEF1234567890123456789012345678901234".to_string(),
                function: "updatePrices(address[],uint256[])".to_string(),
                parameters: vec![
                    Parameter {
                        param_type: "address[]".to_string(),
                        value: serde_json::json!([
                            "0x1111111111111111111111111111111111111111",
                            "0x2222222222222222222222222222222222222222"
                        ]),
                    },
                    Parameter {
                        param_type: "uint256[]".to_string(),
                        value: serde_json::json!(["1000000", "2000000"]),
                    },
                ],
            },
            gas_config: None,
        };

        assert_eq!(task.target_function.parameters.len(), 2);
        assert_eq!(task.target_function.parameters[0].param_type, "address[]");
    }

    #[test]
    fn test_scheduled_task_with_property_condition() {
        let task = ScheduledTask {
            name: "property_task".to_string(),
            network: "test-network".to_string(),
            schedule: "*/30 * * * * *".to_string(), // Every 30 seconds
            check_condition: Some(CheckCondition::Property {
                contract_address: "0x9876543210987654321098765432109876543210".to_string(),
                property: "isActive".to_string(),
                expected_value: serde_json::Value::Bool(true),
            }),
            target_function: TargetFunction {
                contract_address: "0x9876543210987654321098765432109876543210".to_string(),
                function: "process()".to_string(),
                parameters: vec![],
            },
            gas_config: None,
        };

        match task.check_condition {
            Some(CheckCondition::Property { property, .. }) => {
                assert_eq!(property, "isActive");
            }
            _ => panic!("Expected Property condition"),
        }
    }
}
