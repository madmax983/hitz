//! OpenTelemetry initialisation — traces + metrics over OTLP gRPC.
//!
//! Call [`TelemetryGuard::init`] once at daemon startup. Hold the returned
//! guard for the lifetime of the process; dropping it flushes and shuts down
//! both the tracer and meter providers.
//!
//! When `endpoint` is `None` the function returns immediately with a no-op
//! guard — no global providers are installed and all OpenTelemetry calls are
//! no-ops.

use opentelemetry::trace::TracerProvider as _;
use opentelemetry_otlp::{MetricExporter, SpanExporter, WithExportConfig as _};
use opentelemetry_sdk::metrics::SdkMeterProvider;
use opentelemetry_sdk::trace::SdkTracerProvider;

/// RAII guard that shuts down OpenTelemetry providers on drop.
///
/// Holds `Option<…>` so the no-op path occupies zero heap.
pub struct TelemetryGuard {
    tracer_provider: Option<SdkTracerProvider>,
    meter_provider: Option<SdkMeterProvider>,
}

impl TelemetryGuard {
    /// Initialise OpenTelemetry with an OTLP gRPC exporter.
    ///
    /// If `endpoint` is `None`, returns a no-op guard without installing any
    /// global provider.  The gRPC channel is lazy — init succeeds even when
    /// no collector is running.
    ///
    /// # Errors (logged, not propagated)
    ///
    /// If provider construction fails (e.g. bad endpoint URL), a warning is
    /// logged and the no-op guard is returned so the daemon still starts.
    ///
    /// ## Examples
    ///
    /// ```rust,no_run
    /// use hitz_daemon::TelemetryGuard;
    ///
    /// let guard = TelemetryGuard::init(Some("http://127.0.0.1:4317".to_string()));
    /// // OpenTelemetry tracing and metrics are now active for the scope of `guard`.
    /// ```
    #[must_use]
    pub fn init(endpoint: Option<String>) -> Self {
        let Some(endpoint) = endpoint else {
            return Self {
                tracer_provider: None,
                meter_provider: None,
            };
        };

        // ── Tracer provider ──────────────────────────────────────────────────
        let span_exporter = match SpanExporter::builder()
            .with_tonic()
            .with_endpoint(&endpoint)
            .build()
        {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!("OTel span exporter init failed: {e}; running without traces");
                return Self {
                    tracer_provider: None,
                    meter_provider: None,
                };
            }
        };

        let tracer_provider = SdkTracerProvider::builder()
            .with_batch_exporter(span_exporter)
            .build();

        opentelemetry::global::set_tracer_provider(tracer_provider.clone());

        // ── Meter provider ───────────────────────────────────────────────────
        let metric_exporter = match MetricExporter::builder()
            .with_tonic()
            .with_endpoint(&endpoint)
            .build()
        {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!("OTel metric exporter init failed: {e}; running without metrics");
                return Self {
                    tracer_provider: Some(tracer_provider),
                    meter_provider: None,
                };
            }
        };

        let meter_provider = SdkMeterProvider::builder()
            .with_periodic_exporter(metric_exporter)
            .build();

        opentelemetry::global::set_meter_provider(meter_provider.clone());

        tracing::info!(%endpoint, "OTel OTLP exporter initialised");

        Self {
            tracer_provider: Some(tracer_provider),
            meter_provider: Some(meter_provider),
        }
    }

    /// Build a `tracing_opentelemetry` layer backed by the installed tracer.
    ///
    /// Returns `None` when OpenTelemetry is disabled (no-op path). Call this after
    /// [`TelemetryGuard::init`] and before building the subscriber registry.
    ///
    /// ## Examples
    ///
    /// ```rust,no_run
    /// use hitz_daemon::TelemetryGuard;
    /// use tracing_subscriber::{layer::SubscriberExt, Registry};
    ///
    /// let guard = TelemetryGuard::init(Some("http://127.0.0.1:4317".to_string()));
    /// if let Some(layer) = guard.tracing_layer() {
    ///     let subscriber = Registry::default().with(layer);
    ///     tracing::subscriber::set_global_default(subscriber).unwrap();
    /// }
    /// ```
    #[must_use]
    pub fn tracing_layer(
        &self,
    ) -> Option<
        tracing_opentelemetry::OpenTelemetryLayer<
            tracing_subscriber::Registry,
            opentelemetry_sdk::trace::Tracer,
        >,
    > {
        self.tracer_provider.as_ref().map(|tp| {
            let tracer = tp.tracer("hitz");
            tracing_opentelemetry::layer().with_tracer(tracer)
        })
    }
}

impl Drop for TelemetryGuard {
    fn drop(&mut self) {
        if let Some(ref mp) = self.meter_provider {
            if let Err(e) = mp.shutdown() {
                tracing::warn!("OpenTelemetry meter provider shutdown error: {e}");
            }
        }
        if let Some(ref tp) = self.tracer_provider {
            if let Err(e) = tp.shutdown() {
                tracing::warn!("OpenTelemetry tracer provider shutdown error: {e}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_none_is_noop() {
        let guard = TelemetryGuard::init(None);
        drop(guard); // must not panic
    }

    #[tokio::test]
    async fn init_unreachable_endpoint_is_ok() {
        // gRPC is lazy-connecting; construction succeeds even with nothing
        // listening. Use a port unlikely to be occupied.
        // A Tokio runtime context is required because tonic builds a lazy channel
        // which registers work with the reactor on construction.
        let guard = TelemetryGuard::init(Some("http://127.0.0.1:14317".to_string()));
        drop(guard); // graceful shutdown, must not panic
    }
}
