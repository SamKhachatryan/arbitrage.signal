#[macro_export]
macro_rules! define_prometheus_counter {
    ($name:ident, $metric_name:expr, $description:expr) => {
        lazy_static::lazy_static! {
            pub static ref $name: prometheus::Counter = {
                let opts = prometheus::Opts::new($metric_name, $description);
                let counter = prometheus::Counter::with_opts(opts)
                    .expect("Failed to create counter");
                METRIC_REGISTRY
                    .register(Box::new(counter.clone()))
                    .expect("Failed to register counter");
                counter
            };
        }
    };
}
