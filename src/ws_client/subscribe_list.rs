#[macro_export]
macro_rules! subscribe_list {
    ($state:expr, $pair:expr, [ $( $svc:ident ),* ]) => {
        {
            use futures::future::{join_all, BoxFuture};

            let futs: Vec<BoxFuture<'static, ()>> = vec![
                $(
                    {
                        let cloned = Arc::clone(&$state);
                        $svc::subscribe(cloned, $pair.to_string()).boxed()
                    },
                )*
            ];
            join_all(futs).await;
        }
    };
}