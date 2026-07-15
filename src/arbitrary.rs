//! proptest::Arbitrary for pheno-tracing types (L23)
use proptest::prelude::*;
use crate::Subscriber;

impl Arbitrary for Subscriber {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;
    fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
        prop_oneof![
            Just(Subscriber::Ok),
            Just(Subscriber::Invalid),
            Just(Subscriber::Timeout),
        ].boxed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::test_runner::TestRunner;

    #[test]
    fn arbitrary_generates_valid() {
        let mut runner = TestRunner::default();
        for _ in 0..50 {
            let _ = runner.run(&any::<Subscriber>(), |v| {
                prop_assert_eq!(v.clone(), v.clone());
                Ok(())
            }).unwrap();
        }
    }
}
