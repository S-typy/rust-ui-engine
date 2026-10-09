use std::collections::HashMap;

/// An application-defined stable command name, independent of widget identity.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CommandId(pub String);

impl From<&str> for CommandId {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}
impl From<String> for CommandId {
    fn from(value: String) -> Self {
        Self(value)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandInvocation {
    pub id: CommandId,
    pub checked: Option<bool>,
}

struct Command {
    label: String,
    enabled: bool,
    checked: Option<bool>,
    can_execute: Box<dyn Fn() -> bool>,
    execute: Option<Box<dyn FnMut()>>,
}

/// Registry shared by Ribbon items, menus, and ordinary controls.
#[derive(Default)]
pub struct CommandRegistry {
    commands: HashMap<CommandId, Command>,
    invocations: Vec<CommandInvocation>,
}

impl CommandRegistry {
    pub fn register(&mut self, id: impl Into<CommandId>, label: impl Into<String>) {
        self.commands.insert(
            id.into(),
            Command {
                label: label.into(),
                enabled: true,
                checked: None,
                can_execute: Box::new(|| true),
                execute: None,
            },
        );
    }
    pub fn contains(&self, id: &CommandId) -> bool {
        self.commands.contains_key(id)
    }
    pub fn label(&self, id: &CommandId) -> Option<&str> {
        self.commands.get(id).map(|c| c.label.as_str())
    }
    pub fn enabled(&self, id: &CommandId) -> bool {
        self.commands
            .get(id)
            .is_some_and(|c| c.enabled && (c.can_execute)())
    }
    pub fn checked(&self, id: &CommandId) -> Option<bool> {
        self.commands.get(id).and_then(|c| c.checked)
    }
    pub fn set_enabled(&mut self, id: &CommandId, enabled: bool) -> bool {
        if let Some(command) = self.commands.get_mut(id) {
            command.enabled = enabled;
            true
        } else {
            false
        }
    }
    pub fn set_checked(&mut self, id: &CommandId, checked: Option<bool>) -> bool {
        if let Some(command) = self.commands.get_mut(id) {
            command.checked = checked;
            true
        } else {
            false
        }
    }
    pub fn set_can_execute(&mut self, id: &CommandId, check: impl Fn() -> bool + 'static) -> bool {
        if let Some(command) = self.commands.get_mut(id) {
            command.can_execute = Box::new(check);
            true
        } else {
            false
        }
    }
    pub fn on_execute(&mut self, id: &CommandId, action: impl FnMut() + 'static) -> bool {
        if let Some(command) = self.commands.get_mut(id) {
            command.execute = Some(Box::new(action));
            true
        } else {
            false
        }
    }
    /// Returns false for missing/disabled commands without emitting an invocation.
    pub fn execute(&mut self, id: &CommandId) -> bool {
        if !self.enabled(id) {
            return false;
        }
        let Some(command) = self.commands.get_mut(id) else {
            return false;
        };
        if let Some(action) = &mut command.execute {
            action();
        }
        self.invocations.push(CommandInvocation {
            id: id.clone(),
            checked: command.checked,
        });
        true
    }
    pub fn drain_invocations(&mut self) -> Vec<CommandInvocation> {
        std::mem::take(&mut self.invocations)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, rc::Rc};
    #[test]
    fn can_execute_gates_all_invocation_paths() {
        let enabled = Rc::new(Cell::new(false));
        let gate = Rc::clone(&enabled);
        let mut commands = CommandRegistry::default();
        let id = CommandId::from("save");
        commands.register(id.clone(), "Save");
        commands.set_can_execute(&id, move || gate.get());
        assert!(!commands.execute(&id));
        assert!(commands.drain_invocations().is_empty());
        enabled.set(true);
        commands.set_checked(&id, Some(true));
        assert!(commands.execute(&id));
        assert_eq!(
            commands.drain_invocations(),
            vec![CommandInvocation {
                id: id.clone(),
                checked: Some(true)
            }]
        );
        commands.set_enabled(&id, false);
        assert!(!commands.execute(&id));
    }
}
