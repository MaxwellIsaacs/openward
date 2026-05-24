use askama::Template;
use askama_web::WebTemplate;
use crate::i18n::T;

/// Shared nav context for all page templates.
pub struct NavContext {
    pub operator_name: String,
    pub role: String,
    pub current_path: String,
}

/// Pre-computed flag display data.
pub struct FlagDisplay {
    pub label: String,
    pub css_class: String,
}

/// Pre-computed court date display for calendar.
pub struct CourtDateDisplay {
    pub id: String,
    pub detainee_id: String,
    pub detainee_name: String,
    pub scheduled_date: String,
    pub court_name: String,
    pub purpose: String,
    pub has_outcome: bool,
    pub outcome_summary: String,
}

/// Pre-computed detainee summary for population list.
pub struct DetaineeSummaryDisplay {
    pub id: String,
    pub name: String,
    pub sex: String,
    pub detention_basis: String,
    pub intake_date: String,
    pub days_held: u32,
    pub next_court_date: Option<String>,
    pub flags: Vec<String>,
}

/// Pre-computed detainee detail display.
pub struct DetaineeDisplay {
    pub id: String,
    pub surname: String,
    pub given_names: String,
    pub preferred_name: Option<String>,
    pub sex: String,
    pub date_of_birth: Option<String>,
    pub estimated_age: Option<u32>,
    pub nationality: Option<String>,
    pub national_id: Option<String>,
    pub languages: Vec<String>,
    pub detention_basis_label: String,
    pub intake_date: String,
    pub facility_status: String,
    pub is_released: bool,
    pub housing_unit: Option<String>,
    pub legal_representative: Option<LegalRepDisplay>,
    pub warrants: Vec<WarrantDisplay>,
    pub court_dates: Vec<CourtDateDetailDisplay>,
    pub notes: Vec<NoteDisplay>,
    pub emergency_contacts: Vec<ContactDisplay>,
    pub property: Vec<PropertyDisplay>,
}

pub struct LegalRepDisplay {
    pub name: String,
    pub contact: String,
}

pub struct WarrantDisplay {
    pub issuing_authority: String,
    pub order_type: String,
    pub date_issued: String,
    pub valid_until: Option<String>,
    pub is_active: bool,
}

pub struct CourtDateDetailDisplay {
    pub id: String,
    pub scheduled_date: String,
    pub court_name: String,
    pub purpose: String,
    pub purpose_code: String,
    pub outcome: Option<String>,
    pub has_outcome: bool,
}

pub struct NoteDisplay {
    pub id: i64,
    pub timestamp: String,
    pub author: String,
    pub content: String,
    pub note_type: String,
}

pub struct ContactDisplay {
    pub name: String,
    pub relationship: String,
    pub phone: String,
}

pub struct PropertyDisplay {
    pub id: i64,
    pub description: String,
    pub quantity: u32,
    pub status: String,
    pub returned: bool,
}

/// Pre-computed housing unit display.
pub struct HousingUnitDisplay {
    pub id: String,
    pub name: String,
    pub capacity: u32,
    pub designated_sex: Option<String>,
}

/// Pre-computed daily count display.
pub struct DailyCountDisplay {
    pub opening_count: u32,
    pub admissions: u32,
    pub releases: u32,
    pub transfers_out: u32,
    pub to_court: u32,
    pub to_hospital: u32,
    pub escapes: u32,
    pub deaths: u32,
    pub computed_closing: u32,
    pub actual_closing_count: Option<u32>,
    pub is_balanced: Option<bool>,
    pub finalized_by: Option<String>,
    pub unit_counts: Vec<UnitCountDisplay>,
}

pub struct UnitCountDisplay {
    pub unit_id: String,
    pub count: u32,
}

/// Housing unit paired with its headcount for the daily count page.
pub struct HousingUnitWithCount {
    pub id: String,
    pub name: String,
    pub count: Option<u32>,
}

/// Outcome form display with pre-computed strings.
pub struct OutcomeFormDisplay {
    pub id: String,
    pub scheduled_date: String,
    pub court_name: String,
    pub purpose: String,
}

// === Login ===

#[derive(Template, WebTemplate)]
#[template(path = "login.html")]
pub struct LoginTemplate {
    pub error: Option<String>,
    pub languages: &'static [(&'static str, &'static str)],
    pub t: T,
}

// === Profile ===

/// Display for an active session row (for admin session list).
#[allow(dead_code)]
pub struct SessionDisplay {
    pub id: String,
    pub display_name: String,
    pub role: String,
    pub created_at: String,
    pub expires_at: String,
    pub is_current: bool,
}

#[derive(Template, WebTemplate)]
#[template(path = "profile.html")]
pub struct ProfileTemplate {
    pub nav: NavContext,
    pub display_name: String,
    pub role: String,
    pub language: String,
    pub languages: &'static [(&'static str, &'static str)],
    pub is_admin: bool,
    pub sessions: Vec<SessionDisplay>,
    pub flash: Option<String>,
    pub flash_error: Option<String>,
    pub must_change_password: bool,
    pub t: T,
}

// === Error ===

#[derive(Template, WebTemplate)]
#[template(path = "error.html")]
pub struct ErrorTemplate {
    pub nav: Option<NavContext>,
    pub status: u16,
    pub message: String,
    pub t: T,
}

// === Dashboard ===

#[derive(Template, WebTemplate)]
#[template(path = "dashboard.html")]
pub struct DashboardTemplate {
    pub nav: NavContext,
    pub total_population: u32,
    pub facility_capacity: u32,
    pub occupancy_percent: u32,
    pub pretrial_percent: u32,
    pub as_of: String,
    pub system_warning: Option<String>,
    pub count_status: String,
    pub count_detail: Option<String>,
    pub count_badge_class: String,
    pub basis_breakdown: Vec<BasisSegment>,
    pub alerts: Vec<AlertDisplay>,
    pub t: T,
}

pub struct BasisSegment {
    pub label_key: String,
    pub count: u32,
    pub css_class: String,
    pub link: String,
}

pub struct AlertDisplay {
    pub count: u32,
    pub label_key: String,
    pub css_class: String,
    pub link: String,
}

// === Population List ===

/// Current filter state for pre-filling form controls.
#[derive(Default, Clone)]
pub struct FilterState {
    pub name: String,
    pub basis: String,
    pub bail: String,
    pub severity: String,
    pub sex: String,
    pub has_court_date: String,
    pub has_legal_rep: String,
    pub held_min: String,
    pub held_max: String,
    pub court_overdue: String,
    pub release_overdue: String,
    pub release_within: String,
    pub flag: String,
    pub sort: String,
}

#[derive(Template, WebTemplate)]
#[template(path = "population/page.html")]
pub struct PopulationPageTemplate {
    pub nav: NavContext,
    pub detainees: Vec<DetaineeSummaryDisplay>,
    pub total_matching: u32,
    pub page: u32,
    pub total_pages: u32,
    pub filters: FilterState,
    pub t: T,
}

#[derive(Template, WebTemplate)]
#[template(path = "population/table_body.html")]
pub struct PopulationTableFragment {
    pub detainees: Vec<DetaineeSummaryDisplay>,
    pub total_matching: u32,
    pub page: u32,
    pub total_pages: u32,
    pub t: T,
}

// === Detainee Detail ===

#[derive(Template, WebTemplate)]
#[template(path = "detainee/page.html")]
pub struct DetaineeDetailTemplate {
    pub nav: NavContext,
    pub detainee: DetaineeDisplay,
    pub flags: Vec<FlagDisplay>,
    pub can_delete_notes: bool,
    pub can_add_notes: bool,
    pub audit_entries: Vec<AuditEntryDisplay>,
    pub show_audit: bool,
    pub t: T,
}

#[derive(Template, WebTemplate)]
#[template(path = "detainee/basis_form.html")]
pub struct BasisFormFragment {
    pub detainee_id: String,
    pub current_basis: String,
    pub t: T,
}

#[derive(Template, WebTemplate)]
#[template(path = "detainee/housing_form.html")]
pub struct HousingFormFragment {
    pub detainee_id: String,
    pub housing_units: Vec<HousingUnitDisplay>,
    pub current_unit: Option<String>,
    pub t: T,
}

#[derive(Template, WebTemplate)]
#[template(path = "detainee/court_date_form.html")]
pub struct CourtDateFormFragment {
    pub detainee_id: String,
    pub t: T,
}

#[derive(Template, WebTemplate)]
#[template(path = "detainee/release_form.html")]
pub struct ReleaseFormFragment {
    pub detainee_id: String,
    pub t: T,
}

#[derive(Template, WebTemplate)]
#[template(path = "detainee/note_form.html")]
pub struct NoteFormFragment {
    pub detainee_id: String,
    pub t: T,
}

#[derive(Template, WebTemplate)]
#[template(path = "detainee/property_form.html")]
pub struct PropertyFormFragment {
    pub detainee_id: String,
    pub t: T,
}

// === Admission ===

#[derive(Template, WebTemplate)]
#[template(path = "admission/page.html")]
pub struct AdmissionPageTemplate {
    pub nav: NavContext,
    pub t: T,
}

#[derive(Template, WebTemplate)]
#[template(path = "admission/basis_fields.html")]
pub struct BasisFieldsFragment {
    pub basis_type: String,
    pub t: T,
}

// === Daily Count ===

#[derive(Template, WebTemplate)]
#[template(path = "daily_count/page.html")]
pub struct DailyCountPageTemplate {
    pub nav: NavContext,
    pub count: Option<DailyCountDisplay>,
    pub date: String,
    pub units_with_counts: Vec<HousingUnitWithCount>,
    pub t: T,
}

// === Housing Management ===

/// Pre-computed housing unit display with occupancy.
pub struct HousingUnitFullDisplay {
    pub id: String,
    pub name: String,
    pub capacity: u32,
    pub occupancy: u32,
    pub unit_type: String,
    pub unit_type_code: String,
    pub designated_sex: Option<String>,
    pub designated_sex_code: Option<String>,
    pub designated_age_group: Option<String>,
    pub designated_age_group_code: Option<String>,
}

#[derive(Template, WebTemplate)]
#[template(path = "housing/page.html")]
pub struct HousingPageTemplate {
    pub nav: NavContext,
    pub units: Vec<HousingUnitFullDisplay>,
    pub total_capacity: u32,
    pub total_occupancy: u32,
    pub t: T,
}

#[derive(Template, WebTemplate)]
#[template(path = "housing/create_form.html")]
pub struct HousingCreateFormFragment {
    pub t: T,
}

#[derive(Template, WebTemplate)]
#[template(path = "housing/edit_form.html")]
pub struct HousingEditFormFragment {
    pub unit: HousingUnitFullDisplay,
    pub t: T,
}

// === Operator Management ===

pub struct OperatorDisplay {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub role: String,
    pub role_label: String,
    pub is_active: bool,
    pub last_login: Option<String>,
}

#[derive(Template, WebTemplate)]
#[template(path = "operators/page.html")]
pub struct OperatorsPageTemplate {
    pub nav: NavContext,
    pub operators: Vec<OperatorDisplay>,
    pub flash: Option<String>,
    pub t: T,
}

#[derive(Template, WebTemplate)]
#[template(path = "operators/new.html")]
pub struct OperatorNewTemplate {
    pub nav: NavContext,
    pub languages: &'static [(&'static str, &'static str)],
    pub error: Option<String>,
    pub t: T,
}

#[derive(Template, WebTemplate)]
#[template(path = "operators/edit.html")]
pub struct OperatorEditTemplate {
    pub nav: NavContext,
    pub operator: OperatorDisplay,
    pub languages: &'static [(&'static str, &'static str)],
    pub current_language: String,
    pub error: Option<String>,
    pub t: T,
}

#[derive(Template, WebTemplate)]
#[template(path = "operators/reset_password.html")]
pub struct OperatorResetPasswordTemplate {
    pub nav: NavContext,
    pub operator_id: String,
    pub operator_name: String,
    pub error: Option<String>,
    pub t: T,
}

// === Audit Trail ===

/// Pre-computed audit entry display for the audit log table.
pub struct AuditEntryDisplay {
    pub id: String,
    pub timestamp: String,
    pub operator_name: String,
    pub module: String,
    pub action: String,
    pub target_id: Option<String>,
    pub target_name: Option<String>,
    pub details: String,
}

/// Operator option for filter dropdowns.
pub struct OperatorOption {
    pub id: String,
    pub display_name: String,
}

/// Chain verification result for display.
pub struct VerificationDisplay {
    pub total_entries: u64,
    pub total_epochs: u64,
    pub is_valid: bool,
    pub breaks: Vec<BreakDisplay>,
}

/// A single chain break for display.
pub struct BreakDisplay {
    pub entry_index: u64,
    pub entry_id: String,
    pub epoch: u64,
    pub break_type: String,
}

#[derive(Template, WebTemplate)]
#[template(path = "audit/page.html")]
pub struct AuditPageTemplate {
    pub nav: NavContext,
    pub entries: Vec<AuditEntryDisplay>,
    pub operators: Vec<OperatorOption>,
    pub total_count: u32,
    pub page: u32,
    pub total_pages: u32,
    pub t: T,
}

#[derive(Template, WebTemplate)]
#[template(path = "audit/table_body.html")]
pub struct AuditTableFragment {
    pub entries: Vec<AuditEntryDisplay>,
    pub total_count: u32,
    pub page: u32,
    pub total_pages: u32,
    pub t: T,
}

#[derive(Template, WebTemplate)]
#[template(path = "audit/verify.html")]
pub struct AuditVerifyTemplate {
    pub nav: NavContext,
    pub result: VerificationDisplay,
    pub t: T,
}

/// A single key/value row rendered inside the audit detail <dl>.
pub struct AuditDetailField {
    pub label: String,
    pub value: String,
}

/// Full display for one audit entry (detail view).
pub struct AuditDetailDisplay {
    pub id: String,
    pub timestamp: String,
    pub operator_name: String,
    pub operator_id: String,
    pub module: String,
    pub action_raw: String,
    pub action_label: String,
    pub summary: String,
    pub target_id: Option<String>,
    pub target_name: Option<String>,
    pub epoch: u64,
    pub before_fields: Vec<AuditDetailField>,
    pub after_fields: Vec<AuditDetailField>,
    pub self_hash_hex: String,
    pub chain_hash_hex: String,
    pub self_hash_verified: bool,
    pub chain_verified: bool,
}

#[derive(Template, WebTemplate)]
#[template(path = "audit/detail.html")]
pub struct AuditDetailTemplate {
    pub nav: NavContext,
    pub entry: AuditDetailDisplay,
    pub t: T,
}

// === Backup & Export ===

#[derive(Template, WebTemplate)]
#[template(path = "backup/page.html")]
pub struct BackupPageTemplate {
    pub nav: NavContext,
    pub last_backup: Option<String>,
    pub default_from: String,
    pub default_to: String,
    pub t: T,
}

// === Court Calendar ===

#[derive(Template, WebTemplate)]
#[template(path = "court_calendar/page.html")]
pub struct CourtCalendarPageTemplate {
    pub nav: NavContext,
    pub upcoming: Vec<CourtDateDisplay>,
    pub past: Vec<CourtDateDisplay>,
    pub t: T,
}

#[derive(Template, WebTemplate)]
#[template(path = "court_calendar/outcome_form.html")]
pub struct OutcomeFormFragment {
    pub court_date: OutcomeFormDisplay,
    pub t: T,
}

// === Transfer ===

#[derive(Template, WebTemplate)]
#[template(path = "detainee/transfer_form.html")]
pub struct TransferFormFragment {
    pub detainee_id: String,
    pub t: T,
}

/// Pre-computed transfer display for the transfers list.
pub struct TransferDisplay {
    pub date: String,
    pub detainee_name: String,
    pub detainee_id: String,
    pub destination: String,
    pub reason: String,
    pub authorized_by: String,
}

#[derive(Template, WebTemplate)]
#[template(path = "transfers/page.html")]
pub struct TransfersPageTemplate {
    pub nav: NavContext,
    pub transfers: Vec<TransferDisplay>,
    pub t: T,
}

// === Batch Admission ===

#[derive(Template, WebTemplate)]
#[template(path = "admission/batch.html")]
pub struct BatchAdmissionPageTemplate {
    pub nav: NavContext,
    pub t: T,
}

// === Court Date Edit ===

#[derive(Template, WebTemplate)]
#[template(path = "detainee/edit_court_date_form.html")]
pub struct EditCourtDateFormFragment {
    pub detainee_id: String,
    pub court_date_id: String,
    pub scheduled_date: String,
    pub court_name: String,
    pub purpose_code: String,
    pub t: T,
}
