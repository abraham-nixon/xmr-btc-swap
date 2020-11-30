use anyhow::Result;
use structopt::StructOpt;
use swap::{alice::swap::swap, bob::swap::BobState, cli::Options, storage::Database};

#[tokio::main]
async fn main() -> Result<()> {
    let opt = Options::from_args();

    let db = Database::open(std::path::Path::new("./.swap-db/")).unwrap();
    let swarm = unimplemented!();
    let bitcoin_wallet = unimplemented!();
    let monero_wallet = unimplemented!();
    let mut rng = unimplemented!();
    let bob_state = unimplemented!();

    match opt {
        Options::Alice { .. } => {
            swap(bob_state, swarm, bitcoin_wallet, monero_wallet).await?;
        }
        Options::Recover { .. } => {
            let _stored_state: BobState = unimplemented!("io.get_state(uuid)?");
            // abort(_stored_state, _io);
        }
        _ => {}
    };
}
