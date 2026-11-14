//! Responsibility: protected backup and fresh restore use-case ordering.
//! Dependencies: pure backup policy and purpose-owned capabilities only.
//! Callers: compatibility backup composition; invariants: publish completion last, stop on error.
use super::backup_domain::{Limits, Manifest, validate_manifest};
use super::backup_ports::{FreshRestore, ProtectedBackup};
pub(crate) fn create<P: ProtectedBackup>(port: &mut P) -> Result<Manifest, P::Error> {
    let (database, objects) = port.snapshot()?;
    port.copy_objects(&objects)?;
    let manifest = Manifest {
        format: 1,
        schema: 6,
        producer: env!("CARGO_PKG_VERSION").into(),
        database,
        objects,
    };
    port.publish(&manifest)?;
    Ok(manifest)
}
pub(crate) fn restore<P: FreshRestore>(port: &mut P, limits: Limits) -> Result<Manifest, P::Error> {
    let manifest = port.read_manifest()?;
    validate_manifest(&manifest, limits)?;
    port.verify_snapshot(&manifest)?;
    port.copy_snapshot(&manifest)?;
    port.copy_objects(&manifest.objects)?;
    port.publish_restored()?;
    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::backup_domain::{Object, PolicyError, account_bytes, object_identity};
    #[derive(Debug, PartialEq, Eq)]
    enum Error {
        Policy(PolicyError),
        Cancelled,
    }
    impl From<PolicyError> for Error {
        fn from(e: PolicyError) -> Self {
            Self::Policy(e)
        }
    }
    struct Fake {
        calls: Vec<&'static str>,
        fail: Option<&'static str>,
        schema: i64,
    }
    impl Fake {
        fn call(&mut self, name: &'static str) -> Result<(), Error> {
            self.calls.push(name);
            if self.fail == Some(name) {
                Err(Error::Cancelled)
            } else {
                Ok(())
            }
        }
    }
    fn object() -> Object {
        Object {
            hash: "a".repeat(64),
            bytes: 1,
        }
    }
    impl ProtectedBackup for Fake {
        type Error = Error;
        fn snapshot(&mut self) -> Result<(Object, Vec<Object>), Error> {
            self.call("snapshot")?;
            Ok((object(), vec![object()]))
        }
        fn copy_objects(&mut self, objects: &[Object]) -> Result<(), Error> {
            assert_eq!(objects, &[object()]);
            self.call("objects")
        }
        fn publish(&mut self, m: &Manifest) -> Result<(), Error> {
            assert_eq!((m.format, m.schema), (1, 6));
            assert_eq!(m.database, object());
            self.call("publish")
        }
    }
    impl FreshRestore for Fake {
        type Error = Error;
        fn read_manifest(&mut self) -> Result<Manifest, Error> {
            self.call("manifest")?;
            Ok(Manifest {
                format: 1,
                schema: self.schema,
                producer: "original".into(),
                database: object(),
                objects: vec![object()],
            })
        }
        fn verify_snapshot(&mut self, _: &Manifest) -> Result<(), Error> {
            self.call("verify")
        }
        fn copy_snapshot(&mut self, _: &Manifest) -> Result<(), Error> {
            self.call("database")
        }
        fn copy_objects(&mut self, objects: &[Object]) -> Result<(), Error> {
            assert_eq!(objects, &[object()]);
            self.call("objects")
        }
        fn publish_restored(&mut self) -> Result<(), Error> {
            self.call("publish")
        }
    }
    #[test]
    fn protected_backup_stops_at_each_failure_and_publishes_last() {
        let order = ["snapshot", "objects", "publish"];
        for i in 0..=order.len() {
            let mut port = Fake {
                calls: vec![],
                fail: order.get(i).copied(),
                schema: 6,
            };
            let result = create(&mut port);
            assert_eq!(result.is_ok(), i == order.len());
            assert_eq!(port.calls, order[..(i + 1).min(order.len())]);
        }
    }
    #[test]
    fn restore_validates_before_copy_and_stops_on_cancellation() {
        let order = ["manifest", "verify", "database", "objects", "publish"];
        for i in 0..=order.len() {
            let mut port = Fake {
                calls: vec![],
                fail: order.get(i).copied(),
                schema: 6,
            };
            let result = restore(&mut port, Limits::default());
            assert_eq!(result.is_ok(), i == order.len());
            assert_eq!(port.calls, order[..(i + 1).min(order.len())]);
        }
        for schema in [5, 7] {
            let mut port = Fake {
                calls: vec![],
                fail: None,
                schema,
            };
            assert_eq!(
                restore(&mut port, Limits::default()),
                Err(Error::Policy(PolicyError::Invalid(
                    "unsupported format/schema or count"
                )))
            );
            assert_eq!(port.calls, ["manifest"]);
        }
    }
    #[test]
    fn limits_and_registered_identity_policy_preserve_bounds() {
        assert_eq!(account_bytes(4, 6, 10), Ok(10));
        assert_eq!(account_bytes(4, 7, 10), Err(PolicyError::Limit));
        assert_eq!(
            account_bytes(u64::MAX, 1, u64::MAX),
            Err(PolicyError::Limit)
        );
        assert!(object_identity(&"a".repeat(64), 0).is_ok());
        assert!(object_identity(&"A".repeat(64), 0).is_err());
        assert!(object_identity(&"a".repeat(64), -1).is_err());
    }
}
