use dashmap::DashMap;
use once_cell::sync::Lazy;

#[derive(Debug, Clone)]
pub enum WizardStep {
    AskName,
    AskUrl,
    AskType,
    AskKeywords,
    AskSelector,
    AskFilters,
    AskNotifyMode,
    Confirm,
}

#[derive(Debug, Clone)]
pub enum EditStep {
    ChooseField,
    EditName,
    EditUrl,
    EditKeywords,
    EditSelector,
    EditNotifyMode,
    EditFilters,
    Confirm,
}

#[derive(Debug, Clone, Default)]
pub struct WizardData {
    pub name: Option<String>,
    pub url: Option<String>,
    pub search_type: Option<String>,
    pub keywords: Option<String>,
    pub css_selector: Option<String>,
    pub notify_mode: Option<String>,
    pub filters: Option<String>,
}

#[derive(Debug, Clone)]
pub enum WizardMode {
    Create,
    Edit { config_id: i64 },
}

#[derive(Debug, Clone)]
pub struct WizardState {
    pub step: WizardStep,
    pub data: WizardData,
    pub mode: WizardMode,
}

#[derive(Debug, Clone)]
pub struct EditState {
    pub step: EditStep,
    pub config_id: i64,
    pub data: WizardData,
}

static WIZARDS: Lazy<DashMap<i64, WizardState>> = Lazy::new(DashMap::new);
static EDITORS: Lazy<DashMap<i64, EditState>> = Lazy::new(DashMap::new);

pub fn start_wizard(user_id: i64) {
    WIZARDS.insert(
        user_id,
        WizardState {
            step: WizardStep::AskName,
            data: WizardData::default(),
            mode: WizardMode::Create,
        },
    );
}

pub fn start_edit_wizard(user_id: i64, config_id: i64, current_data: WizardData) {
    EDITORS.insert(
        user_id,
        EditState {
            step: EditStep::ChooseField,
            config_id,
            data: current_data,
        },
    );
}

pub fn get_wizard_state(user_id: i64) -> Option<WizardState> {
    WIZARDS.get(&user_id).map(|e| e.clone())
}

pub fn get_edit_state(user_id: i64) -> Option<EditState> {
    EDITORS.get(&user_id).map(|e| e.clone())
}

pub fn update_wizard_data(user_id: i64, data: WizardData) {
    if let Some(mut entry) = WIZARDS.get_mut(&user_id) {
        entry.data = data;
    }
}

pub fn update_edit_data(user_id: i64, data: WizardData) {
    if let Some(mut entry) = EDITORS.get_mut(&user_id) {
        entry.data = data;
    }
}

pub fn set_wizard_step(user_id: i64, step: WizardStep) {
    if let Some(mut entry) = WIZARDS.get_mut(&user_id) {
        entry.step = step;
    }
}

pub fn set_edit_step(user_id: i64, step: EditStep) {
    if let Some(mut entry) = EDITORS.get_mut(&user_id) {
        entry.step = step;
    }
}

pub fn clear_wizard(user_id: i64) {
    WIZARDS.remove(&user_id);
}

pub fn clear_editor(user_id: i64) {
    EDITORS.remove(&user_id);
}
