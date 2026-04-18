use dashmap::DashMap;
use once_cell::sync::Lazy;

#[derive(Debug, Clone)]
pub enum WizardStep {
    AskName,
    AskUrl,
    AskType,
    AskKeywords,
    AskSelector,
    AskNotifyMode,
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
}

#[derive(Debug, Clone)]
pub struct WizardState {
    pub step: WizardStep,
    pub data: WizardData,
}

static WIZARDS: Lazy<DashMap<i64, WizardState>> = Lazy::new(DashMap::new);

pub fn start_wizard(user_id: i64) {
    WIZARDS.insert(
        user_id,
        WizardState {
            step: WizardStep::AskName,
            data: WizardData::default(),
        },
    );
}

pub fn get_wizard_state(user_id: i64) -> Option<WizardState> {
    WIZARDS.get(&user_id).map(|e| e.clone())
}

pub fn update_wizard_data(user_id: i64, data: WizardData) {
    if let Some(mut entry) = WIZARDS.get_mut(&user_id) {
        entry.data = data;
    }
}

pub fn set_wizard_step(user_id: i64, step: WizardStep) {
    if let Some(mut entry) = WIZARDS.get_mut(&user_id) {
        entry.step = step;
    }
}

pub fn clear_wizard(user_id: i64) {
    WIZARDS.remove(&user_id);
}
