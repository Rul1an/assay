use super::super::args::{QuarantineArgs, QuarantineSub};
use crate::exit_codes::EXIT_SUCCESS;

pub(crate) async fn run(args: QuarantineArgs) -> anyhow::Result<i32> {
    super::runner_builder::ensure_parent_dir(&args.db)?;
    let store = assay_core::storage::Store::open(&args.db)?;
    store.init_schema()?;
    let svc = assay_core::quarantine::QuarantineService::new(store);

    match args.cmd {
        QuarantineSub::Add { test_id, reason } => {
            svc.add(&args.suite, &test_id, &reason)?;
            eprintln!("quarantine added: suite={} test_id={}", args.suite, test_id);
        }
        QuarantineSub::Remove { test_id } => {
            svc.remove(&args.suite, &test_id)?;
            eprintln!(
                "quarantine removed: suite={} test_id={}",
                args.suite, test_id
            );
        }
        QuarantineSub::List => {
            eprintln!("quarantine list: not implemented");
        }
    }
    Ok(EXIT_SUCCESS)
}
