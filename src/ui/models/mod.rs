// ui/models/mod.rs - GPUI Models for entity-scoped state
mod status_counts_model;
pub use status_counts_model::StatusCountsModel;

mod notification_panel_model;
pub use notification_panel_model::NotificationPanelModel;

mod new_branch_dialog_model;
pub use new_branch_dialog_model::NewBranchDialogModel;

mod pane_summary_model;
pub use pane_summary_model::{PaneSummaryModel, PaneSummary};
