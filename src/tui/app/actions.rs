use super::{App, AppTab, Focus, InputMode, ServerState};
use crate::tui::action::Action;
use ratatui::widgets::ListState;
use tui_input::Input;

impl App {
    /// Despacha y procesa una acción del sistema actualizando el estado de la aplicación.
    pub fn update(
        &mut self,
        action: Action,
        event_tx: &tokio::sync::mpsc::Sender<crate::tui::event::AppEvent>,
    ) {
        match action {
            Action::Quit => {
                self.abort_console_stream();
                self.should_quit = true;
            }
            Action::FocusSidebar => {
                self.clear_status();
                self.focus = Focus::Sidebar;
            }
            Action::FocusMainPanel => {
                self.clear_status();
                self.focus = Focus::MainPanel;
            }
            Action::ToggleFocus => {
                self.clear_status();
                self.focus = match self.focus {
                    Focus::Sidebar => Focus::MainPanel,
                    Focus::MainPanel => Focus::Sidebar,
                };
            }
            Action::Up => {
                if self.focus == Focus::MainPanel {
                    if self.active_tab == AppTab::Console {
                        let max_scroll = self.console_max_scroll();
                        if max_scroll > 0 {
                            self.console_scroll =
                                self.console_scroll.saturating_add(1).min(max_scroll);
                            self.auto_scroll = false;
                        } else {
                            self.console_scroll = 0;
                        }
                        return;
                    }
                    if self.active_tab == AppTab::Settings {
                        let count = self.filtered_properties().len();
                        if count > 0 {
                            if self.selected_property > 0 {
                                self.selected_property -= 1;
                            } else {
                                self.selected_property = count - 1;
                            }
                            return;
                        }
                    }
                    if self.active_tab == AppTab::Modrinth {
                        if !self.installed_mods.is_empty() {
                            if self.selected_installed_mod > 0 {
                                self.selected_installed_mod -= 1;
                            } else {
                                self.selected_installed_mod = self.installed_mods.len() - 1;
                            }
                        }
                        return;
                    }
                }
                if !self.instances.is_empty() {
                    let prev_idx = self.selected_instance;
                    if self.selected_instance == 0 {
                        self.selected_instance = self.instances.len() - 1;
                    } else {
                        self.selected_instance -= 1;
                    }
                    if prev_idx != self.selected_instance {
                        self.clear_status();
                        self.log_buffer.clear();
                        self.console_scroll = 0;
                        self.auto_scroll = true;
                        self.active_console_instance = None;
                        self.start_console_stream();
                        self.selected_installed_mod = 0;
                        *self.installed_mods_state.borrow_mut() = ListState::default();
                        self.load_properties_for_selected();
                        self.load_installed_mods();
                    }
                }
            }
            Action::Down => {
                if self.focus == Focus::MainPanel {
                    if self.active_tab == AppTab::Console {
                        let max_scroll = self.console_max_scroll();
                        self.console_scroll = self.console_scroll.min(max_scroll).saturating_sub(1);
                        if self.console_scroll == 0 {
                            self.auto_scroll = true;
                        }
                        return;
                    }
                    if self.active_tab == AppTab::Settings {
                        let count = self.filtered_properties().len();
                        if count > 0 {
                            if self.selected_property + 1 < count {
                                self.selected_property += 1;
                            } else {
                                self.selected_property = 0;
                            }
                            return;
                        }
                    }
                    if self.active_tab == AppTab::Modrinth {
                        if !self.installed_mods.is_empty() {
                            if self.selected_installed_mod + 1 < self.installed_mods.len() {
                                self.selected_installed_mod += 1;
                            } else {
                                self.selected_installed_mod = 0;
                                *self.installed_mods_state.borrow_mut() = ListState::default();
                            }
                        }
                        return;
                    }
                }
                if !self.instances.is_empty() {
                    let prev_idx = self.selected_instance;
                    if self.selected_instance + 1 >= self.instances.len() {
                        self.selected_instance = 0;
                    } else {
                        self.selected_instance += 1;
                    }
                    if prev_idx != self.selected_instance {
                        self.clear_status();
                        self.log_buffer.clear();
                        self.console_scroll = 0;
                        self.auto_scroll = true;
                        self.active_console_instance = None;
                        self.start_console_stream();
                        self.selected_installed_mod = 0;
                        *self.installed_mods_state.borrow_mut() = ListState::default();
                        self.load_properties_for_selected();
                        self.load_installed_mods();
                    }
                }
            }
            Action::NextTab => {
                self.clear_status();
                self.properties_search_active = false;
                let idx = self.active_tab as usize;
                let next = (idx + 1) % AppTab::ALL.len();
                self.active_tab = AppTab::ALL[next];
                self.focus = match self.active_tab {
                    AppTab::Dashboard | AppTab::Console => Focus::Sidebar,
                    AppTab::Modrinth | AppTab::Settings => Focus::MainPanel,
                };
                if self.active_tab == AppTab::Console {
                    self.start_console_stream();
                } else if self.active_tab == AppTab::Modrinth {
                    self.load_installed_mods();
                }
            }
            Action::PrevTab => {
                self.clear_status();
                self.properties_search_active = false;
                let idx = self.active_tab as usize;
                let prev = if idx == 0 {
                    AppTab::ALL.len() - 1
                } else {
                    idx - 1
                };
                self.active_tab = AppTab::ALL[prev];
                self.focus = match self.active_tab {
                    AppTab::Dashboard | AppTab::Console => Focus::Sidebar,
                    AppTab::Modrinth | AppTab::Settings => Focus::MainPanel,
                };
                if self.active_tab == AppTab::Console {
                    self.start_console_stream();
                } else if self.active_tab == AppTab::Modrinth {
                    self.load_installed_mods();
                }
            }
            Action::GoToTab(idx) => {
                self.clear_status();
                self.properties_search_active = false;
                if idx < AppTab::ALL.len() {
                    self.active_tab = AppTab::ALL[idx];
                    self.focus = match self.active_tab {
                        AppTab::Dashboard | AppTab::Console => Focus::Sidebar,
                        AppTab::Modrinth | AppTab::Settings => Focus::MainPanel,
                    };
                    if self.active_tab == AppTab::Console {
                        self.start_console_stream();
                    } else if self.active_tab == AppTab::Modrinth {
                        self.load_installed_mods();
                    }
                }
            }
            Action::StartEditProperty => {
                if self.active_tab == AppTab::Settings {
                    let to_edit = self
                        .filtered_properties()
                        .get(self.selected_property)
                        .map(|&(_, (k, v))| (k.clone(), v.clone()));
                    if let Some((k, v)) = to_edit {
                        self.editing_property_key = Some(k);
                        self.editing_property = Some(Input::new(v));
                        self.input_mode = InputMode::Insert;
                    }
                }
            }
            Action::CancelEditProperty => {
                self.editing_property = None;
                self.editing_property_key = None;
                self.input_mode = InputMode::Normal;
            }
            Action::SubmitEditProperty => {
                if let (Some(input), Some(key)) = (
                    self.editing_property.take(),
                    self.editing_property_key.take(),
                ) {
                    let new_val = input.value().to_string();
                    if let Some((_, v)) = self.properties_list.iter_mut().find(|(k, _)| k == &key) {
                        *v = new_val;
                        self.properties_dirty = true;
                    }
                }
                self.input_mode = InputMode::Normal;
            }
            Action::StartPropertiesSearch => {
                self.properties_search_active = true;
            }
            Action::CancelPropertiesSearch => {
                self.properties_search_active = false;
                self.properties_search_input.reset();
                self.selected_property = 0;
            }
            Action::ConfirmPropertiesSearch => {
                self.properties_search_active = false;
                self.selected_property = 0;
            }
            Action::TogglePropertiesShowAll => {
                if self.active_tab == AppTab::Settings {
                    self.properties_show_all = !self.properties_show_all;
                    self.selected_property = 0;
                }
            }
            Action::RefreshInstances => {
                self.reload_instances();
                self.load_properties_for_selected();
                self.set_status("Lista de servidores recargada");
            }
            Action::ToggleServer => {
                self.execute_toggle_server(event_tx);
            }
            Action::SubmitConsoleCommand => {
                let cmd = self.console_input.value().trim().to_string();
                if !cmd.is_empty() {
                    self.console_input.reset();
                    self.send_console_command(&cmd, event_tx);
                }
            }
            Action::ScrollLogUp(lines) => {
                let max_scroll = self.console_max_scroll();
                if max_scroll > 0 {
                    self.console_scroll = self.console_scroll.saturating_add(lines).min(max_scroll);
                    self.auto_scroll = false;
                } else {
                    self.console_scroll = 0;
                }
            }
            Action::ScrollLogDown(lines) => {
                let max_scroll = self.console_max_scroll();
                self.console_scroll = self.console_scroll.min(max_scroll).saturating_sub(lines);
                if self.console_scroll == 0 {
                    self.auto_scroll = true;
                }
            }
            Action::ScrollLogBottom => {
                self.console_scroll = 0;
                self.auto_scroll = true;
            }
            Action::OpenAddModModal => {
                self.show_add_mod_modal = true;
                self.input_mode = InputMode::Insert;
                self.add_mod_input.reset();
                self.add_mod_error = None;
                self.add_mod_loading = false;
            }
            Action::CloseAddModModal => {
                self.show_add_mod_modal = false;
                self.input_mode = InputMode::Normal;
                self.add_mod_error = None;
                self.add_mod_loading = false;
            }
            Action::SubmitAddModUrl => {
                self.execute_add_mod(event_tx);
            }
            Action::PromptDeleteMod => {
                if !self.installed_mods.is_empty() {
                    if let Some(item) = self.installed_mods.get(self.selected_installed_mod) {
                        self.delete_mod_filename = Some(item.file_name.clone());
                        self.show_delete_mod_modal = true;
                    }
                }
            }
            Action::CancelDeleteMod => {
                self.show_delete_mod_modal = false;
                self.delete_mod_filename = None;
            }
            Action::ConfirmDeleteMod => {
                self.show_delete_mod_modal = false;
                if let Some(filename) = self.delete_mod_filename.take() {
                    if let Some(inst) = self.selected() {
                        let subfolder = inst.addons_subfolder();
                        let path = self
                            .paths
                            .instance_dir(&inst.name)
                            .join(subfolder)
                            .join(&filename);
                        if path.exists() {
                            if let Err(e) = std::fs::remove_file(&path) {
                                self.set_status(&format!("Error al eliminar {}: {}", filename, e));
                            } else {
                                self.set_status(&format!(
                                    "✓ Mod '{}' eliminado correctamente",
                                    filename
                                ));
                                self.load_installed_mods();
                            }
                        }
                    }
                }
            }
            Action::ToggleProperty => {
                self.toggle_selected_property();
            }
            Action::SaveProperties => {
                let _ = self.save_properties();
            }
            Action::OpenCreateModal => {
                self.show_create_modal = true;
                self.input_mode = InputMode::Normal;
                self.create_input_backup = None;
                self.create_field_idx = 0;
                self.create_error = None;
                self.create_progress = None;
                self.create_name_input = Input::default();
                self.create_modpack_input = Input::default();
                self.create_modpack_resolving = false;
                self.create_modpack_status = None;
                self.resolved_modpack = None;
                self.create_ram_input = Input::new("4G".to_string());
                self.create_port_input = Input::new("25565".to_string());
                self.load_wizard_versions(event_tx);
            }
            Action::CloseCreateModal => {
                self.show_create_modal = false;
                self.input_mode = InputMode::Normal;
                self.create_input_backup = None;
                self.create_error = None;
                self.create_progress = None;
            }
            Action::NextWizardField => {
                let prev = self.create_field_idx;
                self.create_field_idx = (self.create_field_idx + 1) % 5;
                if self.create_loader_idx == 3 && prev == 2 {
                    self.trigger_resolve_modpack(event_tx);
                }
            }
            Action::PrevWizardField => {
                let prev = self.create_field_idx;
                self.create_field_idx = if self.create_field_idx == 0 {
                    4
                } else {
                    self.create_field_idx - 1
                };
                if self.create_loader_idx == 3 && prev == 2 {
                    self.trigger_resolve_modpack(event_tx);
                }
            }
            Action::SelectWizardLoader(idx) => {
                if idx < 4 {
                    self.create_loader_idx = idx;
                    self.create_version_idx = 0;
                    if idx < 3 {
                        self.load_wizard_versions(event_tx);
                    }
                }
            }
            Action::ResolveModpack => {
                self.trigger_resolve_modpack(event_tx);
            }
            Action::SubmitCreateInstance => {
                self.execute_create_instance(event_tx);
            }
            Action::PromptDeleteInstance => {
                if self.instances.is_empty() {
                    return;
                }
                if let Some(inst) = self.selected() {
                    if inst.state == ServerState::Online
                        || inst.state == ServerState::Starting
                        || inst.state == ServerState::Stopping
                    {
                        self.set_status("Detén el servidor antes de eliminarlo");
                        return;
                    }
                    self.delete_server_name = Some(inst.name.clone());
                    self.show_delete_modal = true;
                }
            }
            Action::CancelDeleteInstance => {
                self.show_delete_modal = false;
                self.delete_server_name = None;
            }
            Action::ConfirmDeleteInstance => {
                self.show_delete_modal = false;
                if let Some(name) = self.delete_server_name.take() {
                    self.execute_delete_instance(&name);
                }
            }
            Action::Resize(w, h) => {
                self.terminal_size = (w, h);
            }
            Action::SetStatus(msg) => {
                self.set_status(&msg);
            }
            _ => {}
        }
    }

}
