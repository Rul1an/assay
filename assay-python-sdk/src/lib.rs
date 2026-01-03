use pyo3::exceptions::{PyFileNotFoundError, PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use serde::Deserialize;

#[pyclass]
struct Policy {
    inner: assay_core::model::Policy,
}

#[pymethods]
impl Policy {
    #[staticmethod]
    fn from_file(path: &str) -> PyResult<Self> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| PyFileNotFoundError::new_err(e.to_string()))?;
        let policy: assay_core::model::Policy =
            serde_yaml::from_str(&content).map_err(|e| PyValueError::new_err(e.to_string()))?;
        Ok(Policy { inner: policy })
    }
}

#[pyclass]
struct CoverageAnalyzer {
    inner: assay_core::coverage::CoverageAnalyzer,
    policy: assay_core::model::Policy,
}

#[derive(Deserialize)]
struct RawToolCall {
    #[serde(alias = "name")]
    tool: Option<String>,
    #[serde(alias = "tool_name")]
    tool_name: Option<String>,
    #[serde(default)]
    args: Option<serde_json::Value>,
    #[serde(alias = "arguments", default)]
    params: Option<serde_json::Value>,
}

#[pymethods]
impl CoverageAnalyzer {
    #[new]
    fn new(policy: &Policy) -> Self {
        CoverageAnalyzer {
            inner: assay_core::coverage::CoverageAnalyzer::from_policy(&policy.inner),
            policy: policy.inner.clone(),
        }
    }

    fn analyze(
        &self,
        traces: Vec<Vec<PyObject>>,
        threshold: f64,
        py: Python<'_>,
    ) -> PyResult<String> {
        let mut records = Vec::new();
        let explainer = assay_core::explain::TraceExplainer::new(self.policy.clone());

        for (i, trace_objs) in traces.iter().enumerate() {
            let mut tool_calls = Vec::new();
            for obj in trace_objs {
                // In PyO3 0.23, PyObject is an alias for Py<PyAny>.
                // To treat it as a reference for pythonize, we need a Bound<'_, PyAny>.
                // obj.bind(py) gives us a Bound<'_, PyAny>.
                let bound = obj.bind(py);
                let raw: RawToolCall = pythonize::depythonize(bound)
                    .map_err(|e| PyValueError::new_err(format!("Invalid trace format: {}", e)))?;

                let tool = raw
                    .tool
                    .or(raw.tool_name)
                    .unwrap_or_else(|| "unknown".to_string());
                let args = raw.args.or(raw.params);

                tool_calls.push(assay_core::explain::ToolCall { tool, args });
            }

            let explanation = explainer.explain(&tool_calls);

            let mut tools_called = Vec::new();
            let mut rules_triggered = std::collections::HashSet::new();

            for step in explanation.steps {
                tools_called.push(step.tool);
                for rule_eval in step.rules_evaluated {
                    rules_triggered.insert(rule_eval.rule_id);
                }
            }

            records.push(assay_core::coverage::TraceRecord {
                trace_id: format!("trace_{}", i),
                tools_called,
                rules_triggered,
            });
        }

        let report = self.inner.analyze(&records, threshold);
        serde_json::to_string(&report).map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }
}

#[pymodule]
#[pyo3(name = "_native")]
fn native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Policy>()?;
    m.add_class::<CoverageAnalyzer>()?;
    m.add_class::<AssayClient>()?;
    Ok(())
}

#[pyclass]
/// A client for recording Assay traces from Python.
struct AssayClient {
    // Thread-safe writer to keep file open and avoid race conditions within the process.
    writer: Option<std::sync::Mutex<std::io::BufWriter<std::fs::File>>>,
}

#[pymethods]
impl AssayClient {
    #[new]
    #[pyo3(signature = (trace_file=None))]
    /// Create a new AssayClient.
    ///
    /// Args:
    ///     trace_file (Optional[str]): Path to the file where traces will be recorded (JSONL).
    ///                                 If None, `record_trace` will raise an error.
    fn new(trace_file: Option<String>) -> PyResult<Self> {
        let writer = if let Some(path_str) = trace_file {
            let path = std::path::Path::new(&path_str);
            let file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .map_err(|e| {
                    PyRuntimeError::new_err(format!("Failed to open trace file: {}", e))
                })?;
            Some(std::sync::Mutex::new(std::io::BufWriter::new(file)))
        } else {
            None
        };
        Ok(AssayClient { writer })
    }

    /// Record a trace object to the configured trace file.
    ///
    /// Args:
    ///     trace (Any): A JSON-serializable object (usually a dict) representing the trace.
    ///
    /// Raises:
    ///     RuntimeError: If no trace_file was configured or writing fails.
    ///     ValueError: If the trace object cannot be serialized to JSON.
    fn record_trace(&self, trace: PyObject, py: Python<'_>) -> PyResult<()> {
        if let Some(mutex) = &self.writer {
            let mut writer = mutex
                .lock()
                .map_err(|_| PyRuntimeError::new_err("Failed to lock trace file writer"))?;

            let bound = trace.bind(py);
            // We validate it is a JSON-serializable object by converting to Value
            let value: serde_json::Value = pythonize::depythonize(bound)
                .map_err(|e| PyValueError::new_err(format!("Invalid trace format: {}", e)))?;

            // Write as JSONL
            use std::io::Write;
            serde_json::to_writer(&mut *writer, &value).map_err(|e| {
                PyRuntimeError::new_err(format!("Failed to serialize trace: {}", e))
            })?;
            writeln!(writer)
                .map_err(|e| PyRuntimeError::new_err(format!("Failed to write newline: {}", e)))?;

            // Flush to ensure data is written immediately (useful for logs/traces)
            writer
                .flush()
                .map_err(|e| PyRuntimeError::new_err(format!("Failed to flush: {}", e)))?;
        } else {
            return Err(PyRuntimeError::new_err(
                "trace_file is not configured; provide trace_file when creating AssayClient to enable record_trace",
            ));
        }
        Ok(())
    }
}
