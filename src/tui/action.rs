#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Quit,
    Tick,
    Render,
    Resize(u16, u16),
    Up,
    Down,
    Left,
    Right,
    Select,
    NextTab,
    PrevTab,
    GoToTab(usize),
    ToggleServer,
    RefreshInstances,
    SetStatus(String),
    FocusSidebar,
    FocusMainPanel,
    ToggleFocus,
    // Consola
    SubmitConsoleCommand,
    ScrollLogUp(usize),
    ScrollLogDown(usize),
    ScrollLogBottom,
    // Modrinth
    SubmitModrinthSearch,
    InstallSelectedMod,
    // Propiedades
    ToggleProperty,
    CycleProperty(i8),
    SaveProperties,
    StartEditProperty,
    CancelEditProperty,
    SubmitEditProperty,
    // Modal Creación
    OpenCreateModal,
    CloseCreateModal,
    NextWizardField,
    PrevWizardField,
    SelectWizardLoader(usize),
    SubmitCreateInstance,
}
