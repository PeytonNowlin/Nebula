use std::time::{Duration, Instant};

use nebula_ast::Span;

use crate::{Runtime, RuntimeError, Value};

pub const AGENT_DEFAULT_MAX_RUNTIME_MS: u64 = 30_000;
pub const AGENT_DEFAULT_MAX_LOOP_ITERATIONS: u64 = 1_000_000;
pub const AGENT_DEFAULT_MAX_MEMORY_BYTES: usize = 64 * 1024 * 1024;

/// Interpreter resource limits. `None` fields are unlimited.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceLimits {
    pub max_runtime: Option<Duration>,
    pub max_loop_iterations: Option<u64>,
    pub max_memory_bytes: Option<usize>,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self::agent_defaults()
    }
}

impl ResourceLimits {
    pub fn agent_defaults() -> Self {
        Self {
            max_runtime: Some(Duration::from_millis(AGENT_DEFAULT_MAX_RUNTIME_MS)),
            max_loop_iterations: Some(AGENT_DEFAULT_MAX_LOOP_ITERATIONS),
            max_memory_bytes: Some(AGENT_DEFAULT_MAX_MEMORY_BYTES),
        }
    }

    pub fn unlimited() -> Self {
        Self {
            max_runtime: None,
            max_loop_iterations: None,
            max_memory_bytes: None,
        }
    }
}

pub(crate) fn value_footprint(value: &Value) -> usize {
    const SCALAR: usize = 8;
    match value {
        Value::Int(_) | Value::Float(_) | Value::Bool(_) | Value::None => SCALAR,
        Value::Str(s) => s.len().saturating_add(SCALAR),
        Value::Some(inner) => SCALAR.saturating_add(value_footprint(inner)),
        Value::List(items) => {
            let mut size = SCALAR.saturating_add(items.len() * 8);
            for item in items {
                size = size.saturating_add(value_footprint(item));
            }
            size
        }
        Value::Map(map) => {
            let mut size = SCALAR.saturating_add(map.len() * 16);
            for (key, val) in map {
                size = size.saturating_add(key.len()).saturating_add(value_footprint(val));
            }
            size
        }
        Value::Struct { name, fields } => {
            let mut size = SCALAR.saturating_add(name.len()).saturating_add(fields.len() * 16);
            for (key, val) in fields {
                size = size.saturating_add(key.len()).saturating_add(value_footprint(val));
            }
            size
        }
    }
}

impl Runtime {
    pub fn with_resource_limits(mut self, limits: ResourceLimits) -> Self {
        self.limits = limits;
        self
    }

    pub(crate) fn begin_run_budget(&mut self) {
        if self.limits.max_runtime.is_some()
            || self.limits.max_loop_iterations.is_some()
            || self.limits.max_memory_bytes.is_some()
        {
            self.started_at = Some(Instant::now());
        }
        self.loop_iterations = 0;
        self.memory_bytes = self.env.values().map(value_footprint).sum();
    }

    pub(crate) fn check_timeout(&self) -> Result<(), RuntimeError> {
        let Some(limit) = self.limits.max_runtime else {
            return Ok(());
        };
        let Some(started) = self.started_at else {
            return Ok(());
        };
        if started.elapsed() > limit {
            return Err(RuntimeError::ExecutionTimeout {
                limit_ms: limit.as_millis() as u64,
            });
        }
        Ok(())
    }

    pub(crate) fn bump_loop_iteration(&mut self, span: Span) -> Result<(), RuntimeError> {
        self.check_timeout()?;
        let Some(limit) = self.limits.max_loop_iterations else {
            return Ok(());
        };
        self.loop_iterations = self.loop_iterations.saturating_add(1);
        if self.loop_iterations > limit {
            return Err(RuntimeError::LoopIterationLimit { limit, span });
        }
        Ok(())
    }

    pub(crate) fn check_transient_allocation(&self, value: &Value) -> Result<(), RuntimeError> {
        let Some(limit) = self.limits.max_memory_bytes else {
            return Ok(());
        };
        let projected = self
            .memory_bytes
            .saturating_add(value_footprint(value));
        if projected > limit {
            return Err(RuntimeError::MemoryLimitExceeded {
                limit_bytes: limit,
                used_bytes: projected,
            });
        }
        Ok(())
    }

    pub(crate) fn check_memory_limit(&self) -> Result<(), RuntimeError> {
        let Some(limit) = self.limits.max_memory_bytes else {
            return Ok(());
        };
        if self.memory_bytes > limit {
            return Err(RuntimeError::MemoryLimitExceeded {
                limit_bytes: limit,
                used_bytes: self.memory_bytes,
            });
        }
        Ok(())
    }

    pub(crate) fn charge_env_binding(
        &mut self,
        name: &str,
        value: Value,
    ) -> Result<(), RuntimeError> {
        if let Some(old) = self.env.get(name) {
            self.memory_bytes = self.memory_bytes.saturating_sub(value_footprint(old));
        }
        self.memory_bytes = self
            .memory_bytes
            .saturating_add(value_footprint(&value));
        self.env.insert(name.to_string(), value);
        self.check_memory_limit()
    }

    pub(crate) fn record_env_footprint_change(
        &mut self,
        name: &str,
        before: usize,
    ) -> Result<(), RuntimeError> {
        let after = self
            .env
            .get(name)
            .map(value_footprint)
            .unwrap_or(0);
        self.memory_bytes = self
            .memory_bytes
            .saturating_sub(before)
            .saturating_add(after);
        self.check_memory_limit()
    }

    pub(crate) fn env_footprint(&self, name: &str) -> usize {
        self.env
            .get(name)
            .map(value_footprint)
            .unwrap_or(0)
    }

    pub(crate) fn take_env_budget(&mut self) -> (std::collections::HashMap<String, Value>, usize) {
        (std::mem::take(&mut self.env), self.memory_bytes)
    }

    pub(crate) fn restore_env_budget(
        &mut self,
        env: std::collections::HashMap<String, Value>,
        memory_bytes: usize,
    ) {
        self.env = env;
        self.memory_bytes = memory_bytes;
    }

    pub(crate) fn finish_value(&self, value: Value) -> Result<Value, RuntimeError> {
        self.check_transient_allocation(&value)?;
        Ok(value)
    }
}