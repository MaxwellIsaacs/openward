use openward_server::auth::{Action, can_do};

// === can_do permission matrix tests ===

#[test]
fn admin_can_do_everything() {
    assert!(can_do("admin", Action::ViewDashboard));
    assert!(can_do("admin", Action::ViewPopulation));
    assert!(can_do("admin", Action::ViewDetaineeDetail));
    assert!(can_do("admin", Action::AdmitDetainee));
    assert!(can_do("admin", Action::UpdateBasisStatusHousing));
    assert!(can_do("admin", Action::ReleaseDetainee));
    assert!(can_do("admin", Action::ManageHousing));
    assert!(can_do("admin", Action::ManageOperators));
    assert!(can_do("admin", Action::ViewAuditTrail));
}

#[test]
fn supervisor_permissions() {
    assert!(can_do("supervisor", Action::ViewDashboard));
    assert!(can_do("supervisor", Action::ViewPopulation));
    assert!(can_do("supervisor", Action::ViewDetaineeDetail));
    assert!(can_do("supervisor", Action::AdmitDetainee));
    assert!(can_do("supervisor", Action::UpdateBasisStatusHousing));
    assert!(can_do("supervisor", Action::ReleaseDetainee));
    assert!(can_do("supervisor", Action::ManageHousing));
    assert!(!can_do("supervisor", Action::ManageOperators));
    assert!(can_do("supervisor", Action::ViewAuditTrail));
}

#[test]
fn operator_permissions() {
    assert!(can_do("operator", Action::ViewDashboard));
    assert!(can_do("operator", Action::ViewPopulation));
    assert!(can_do("operator", Action::ViewDetaineeDetail));
    assert!(can_do("operator", Action::AdmitDetainee));
    assert!(can_do("operator", Action::UpdateBasisStatusHousing));
    assert!(!can_do("operator", Action::ReleaseDetainee));
    assert!(!can_do("operator", Action::ManageHousing));
    assert!(!can_do("operator", Action::ManageOperators));
    assert!(!can_do("operator", Action::ViewAuditTrail));
}

#[test]
fn readonly_permissions() {
    assert!(can_do("readonly", Action::ViewDashboard));
    assert!(can_do("readonly", Action::ViewPopulation));
    assert!(can_do("readonly", Action::ViewDetaineeDetail));
    assert!(!can_do("readonly", Action::AdmitDetainee));
    assert!(!can_do("readonly", Action::UpdateBasisStatusHousing));
    assert!(!can_do("readonly", Action::ReleaseDetainee));
    assert!(!can_do("readonly", Action::ManageHousing));
    assert!(!can_do("readonly", Action::ManageOperators));
    assert!(!can_do("readonly", Action::ViewAuditTrail));
}

#[test]
fn unknown_role_treated_like_operator() {
    // An unknown role gets operator-level permissions (not readonly, not supervisor/admin)
    assert!(can_do("unknown", Action::ViewDashboard));
    assert!(can_do("unknown", Action::AdmitDetainee));
    assert!(!can_do("unknown", Action::ReleaseDetainee));
    assert!(!can_do("unknown", Action::ManageHousing));
    assert!(!can_do("unknown", Action::ManageOperators));
}
