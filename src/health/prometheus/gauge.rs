#[macro_export]
macro_rules! define_prometheus_gauge {
    ($name:ident, $metric_name:expr, $description:expr) => {
        lazy_static::lazy_static! {
            pub static ref $name: prometheus::Gauge = {
                let opts = prometheus::Opts::new($metric_name, $description);
                let gauge = prometheus::Gauge::with_opts(opts)
                    .expect("Failed to create gauge");
                $crate::health::prometheus::registry::METRIC_REGISTRY
                    .register(Box::new(gauge.clone()))
                    .expect("Failed to register gauge");
                gauge
            };
        }
    };
}
