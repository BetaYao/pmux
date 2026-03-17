use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use chrono::Utc;
use gpui::Context;
use tokio::sync::Mutex;
use tokio_cron_scheduler::{Job, JobScheduler};
use uuid::Uuid;
use crate::scheduler::{ScheduledTask, ScheduledTasksStore, TaskRunStatus, default_tasks_path, load_tasks, save_tasks, StorageError};

pub struct SchedulerManager {
    scheduler: Option<Arc<Mutex<JobScheduler>>>,
    tasks: Vec<ScheduledTask>,
    store_path: PathBuf,
    runtime_handle: Option<tokio::runtime::Handle>,
}

impl SchedulerManager {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let store_path = default_tasks_path();
        let tasks = load_tasks(&store_path).unwrap_or_default().tasks;
        
        let mut manager = Self {
            scheduler: None,
            tasks,
            store_path,
            runtime_handle: None,
        };
        
        manager.start_scheduler(cx);
        manager
    }
    
    fn start_scheduler(&mut self, _cx: &mut Context<Self>) {
        let tasks_to_schedule: Vec<ScheduledTask> = self.tasks.iter().filter(|t| t.enabled).cloned().collect();
        
        thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("Failed to create tokio runtime");
            
            rt.block_on(async {
                let scheduler = JobScheduler::new().await.expect("Failed to create job scheduler");
                
                for task in tasks_to_schedule {
                    if let Err(e) = Self::schedule_task_internal(&task, &scheduler).await {
                        eprintln!("Failed to schedule task {}: {}", task.name, e);
                    }
                }
                
                scheduler.start().await.expect("Failed to start scheduler");
                
                // Keep runtime alive
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                }
            });
        });
    }
    
    async fn schedule_task_internal(
        task: &ScheduledTask,
        scheduler: &JobScheduler,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let task_id = task.id;
        let cron = task.cron.clone();
        
        let job = Job::new(&cron, move |_uuid, _l| {
            println!("Task {} triggered at {:?}", task_id, Utc::now());
        })?;
        
        scheduler.add(job).await?;
        Ok(())
    }
    
    pub fn tasks(&self) -> &[ScheduledTask] {
        &self.tasks
    }
    
    pub fn add_task(&mut self, mut task: ScheduledTask, cx: &mut Context<Self>) -> Result<Uuid, StorageError> {
        let id = task.id;
        
        if task.enabled {
            if let Some(ref scheduler) = self.scheduler {
                // Note: This would need to be async to work properly
                // For now, we just store the task
                eprintln!("Note: Task scheduling happens at startup only. Restart to enable new task.");
            }
        }
        
        self.tasks.push(task);
        self.save()?;
        cx.notify();
        
        Ok(id)
    }
    
    pub fn update_task(&mut self, task: ScheduledTask, cx: &mut Context<Self>) -> Result<(), StorageError> {
        if let Some(idx) = self.tasks.iter().position(|t| t.id == task.id) {
            self.tasks[idx] = task;
            self.save()?;
            cx.notify();
        }
        Ok(())
    }
    
    pub fn remove_task(&mut self, id: Uuid, cx: &mut Context<Self>) -> Result<(), StorageError> {
        self.tasks.retain(|t| t.id != id);
        self.save()?;
        cx.notify();
        Ok(())
    }
    
    pub fn toggle_task(&mut self, id: Uuid, cx: &mut Context<Self>) -> Result<(), StorageError> {
        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == id) {
            task.enabled = !task.enabled;
            self.save()?;
            cx.notify();
        }
        Ok(())
    }
    
    pub fn mark_triggered(&mut self, id: Uuid, cx: &mut Context<Self>) -> Result<(), StorageError> {
        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == id) {
            task.last_run = Some(Utc::now());
            task.last_status = Some(TaskRunStatus::Triggered);
            self.save()?;
            cx.notify();
        }
        Ok(())
    }
    
    pub fn mark_failed(&mut self, id: Uuid, cx: &mut Context<Self>) -> Result<(), StorageError> {
        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == id) {
            task.last_status = Some(TaskRunStatus::Failed);
            self.save()?;
            cx.notify();
        }
        Ok(())
    }

    pub fn execute_task(&mut self, task_id: Uuid, cx: &mut Context<Self>) {
        if let Some(task) = self.tasks.iter().find(|t| t.id == task_id).cloned() {
            println!("Executing task: {} - {}", task.name, task.cron);

            // Mark as triggered
            if let Err(e) = self.mark_triggered(task_id, cx) {
                eprintln!("Failed to mark task as triggered: {}", e);
            }

            // TODO: Integrate with RuntimeManager to actually execute the task
            // This requires:
            // 1. Get RuntimeManager reference
            // 2. Switch to/create appropriate pane based on TaskTarget
            // 3. Send command based on TaskType (Agent or Shell)
            // 4. Send notification if configured
            // 5. Handle cleanup if TaskTarget::AutoCreate with cleanup=true
        }
    }

    fn save(&self) -> Result<(), StorageError> {
        let store = ScheduledTasksStore {
            tasks: self.tasks.clone(),
        };
        save_tasks(&self.store_path, &store)
    }
}
