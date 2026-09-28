//! Parameter Undo/Redo history system.
//!
//! Provides granular tracking of continuous user gestures (sliders only commit
//! on gesture release) and atomic compound batch changes (such as applying a preset).

use std::collections::HashMap;

/// An individual parameter value transition
#[derive(Debug, Clone, PartialEq)]
pub struct ParamTransition {
    pub param_id: String,
    pub old_value: f32,
    pub new_value: f32,
}

/// An undoable action
#[derive(Debug, Clone, PartialEq)]
pub enum UndoAction {
    /// Single parameter adjustment (recorded upon gesture finish)
    SingleParam(ParamTransition),
    /// Batch parameter adjustment (e.g. preset application)
    BatchParam {
        name: String,
        changes: Vec<ParamTransition>,
    },
}

#[derive(Debug, Clone)]
pub struct UndoManager {
    undo_stack: Vec<UndoAction>,
    redo_stack: Vec<UndoAction>,
    max_history: usize,
    /// Pending gesture tracking: param_id -> initial value before drag
    active_gestures: HashMap<String, f32>,
}

impl Default for UndoManager {
    fn default() -> Self {
        Self::new(100)
    }
}

impl UndoManager {
    pub fn new(max_history: usize) -> Self {
        Self {
            undo_stack: Vec::with_capacity(max_history),
            redo_stack: Vec::with_capacity(max_history),
            max_history: max_history.max(10),
            active_gestures: HashMap::new(),
        }
    }

    /// Mark the start of a user interaction gesture (mouse press / drag begin).
    /// Saves the initial value for the parameter.
    pub fn begin_gesture(&mut self, param_id: &str, current_val: f32) {
        self.active_gestures
            .entry(param_id.to_string())
            .or_insert(current_val);
    }

    /// Mark the completion of a user interaction gesture (mouse release / drag end).
    /// If the value actually changed, commits a SingleParam undo action.
    pub fn end_gesture(&mut self, param_id: &str, final_val: f32) -> bool {
        if let Some(initial_val) = self.active_gestures.remove(param_id) {
            if (final_val - initial_val).abs() > 1e-6 {
                self.push_action(UndoAction::SingleParam(ParamTransition {
                    param_id: param_id.to_string(),
                    old_value: initial_val,
                    new_value: final_val,
                }));
                return true;
            }
        }
        false
    }

    /// Record an atomic compound batch change (e.g. applying a preset).
    pub fn record_batch(&mut self, name: impl Into<String>, changes: Vec<ParamTransition>) {
        if changes.is_empty() {
            return;
        }
        self.push_action(UndoAction::BatchParam {
            name: name.into(),
            changes,
        });
    }

    fn push_action(&mut self, action: UndoAction) {
        self.undo_stack.push(action);
        if self.undo_stack.len() > self.max_history {
            self.undo_stack.remove(0);
        }
        // Any new action clears the redo history
        self.redo_stack.clear();
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// Undo the most recent action.
    /// Returns the list of `(param_id, target_value)` pairs to restore.
    pub fn undo(&mut self) -> Option<Vec<(String, f32)>> {
        let action = self.undo_stack.pop()?;
        let mut restorations = Vec::new();

        match &action {
            UndoAction::SingleParam(transition) => {
                restorations.push((transition.param_id.clone(), transition.old_value));
            }
            UndoAction::BatchParam { changes, .. } => {
                for change in changes.iter().rev() {
                    restorations.push((change.param_id.clone(), change.old_value));
                }
            }
        }

        self.redo_stack.push(action);
        Some(restorations)
    }

    /// Redo the previously undone action.
    /// Returns the list of `(param_id, target_value)` pairs to restore.
    pub fn redo(&mut self) -> Option<Vec<(String, f32)>> {
        let action = self.redo_stack.pop()?;
        let mut restorations = Vec::new();

        match &action {
            UndoAction::SingleParam(transition) => {
                restorations.push((transition.param_id.clone(), transition.new_value));
            }
            UndoAction::BatchParam { changes, .. } => {
                for change in changes {
                    restorations.push((change.param_id.clone(), change.new_value));
                }
            }
        }

        self.undo_stack.push(action);
        Some(restorations)
    }

    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.active_gestures.clear();
    }
}
