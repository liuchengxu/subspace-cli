mod commands;

use anyhow::Result;
use clap::{Parser, Subcommand};
use codec::Decode;
use subxt::{
    sp_core::{crypto::AccountId32 as AccountId, H256},
    ClientBuilder, DefaultConfig, SubstrateExtrinsicParams,
};

#[subxt::subxt(runtime_metadata_path = "subspace_metadata.scale")]
mod subspace {}

type Balance = u128;
type BlockHash = H256;
type BlockNumber = u32;

type Api = subspace::RuntimeApi<DefaultConfig, SubstrateExtrinsicParams<DefaultConfig>>;

/// Subspace CLI.
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct SubspaceCli {
    /// The websocket url of Subspace node.
    #[clap(long, default_value = "ws://127.0.0.1:9944")]
    pub url: String,

    /// Specify the block number.
    #[clap(long)]
    pub block_number: Option<BlockNumber>,

    /// Specify the block hash.
    #[clap(long)]
    pub block_hash: Option<BlockHash>,

    #[clap(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Snapshot the state for the regenesis purpose.
    Snapshot,
    /// System.
    #[clap(subcommand)]
    System(SystemCommands),
}

#[derive(Debug, Subcommand)]
enum SystemCommands {
    /// Account info.
    Account {
        #[clap(long, parse(try_from_str))]
        who: AccountId,
    },
    /// Events.
    Events,
    /// `BlockHash` mapping.
    BlockHash,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = SubspaceCli::parse();

    let api = ClientBuilder::new()
        .set_url(cli.url)
        .build()
        .await?
        .to_runtime_api::<Api>();

    let maybe_block_hash = if let Some(block_number) = cli.block_number {
        Some(
            api.client
                .rpc()
                .block_hash(Some(block_number.into()))
                .await?
                .unwrap_or_else(|| {
                    panic!("Block hash for block number {} not found", block_number)
                }),
        )
    } else {
        cli.block_hash
    };

    let block_hash = match maybe_block_hash {
        Some(hash) => hash,
        None => api
            .client
            .rpc()
            .block_hash(None)
            .await?
            .expect("Best block hash not found"),
    };

    match cli.command {
        Commands::Snapshot => commands::snapshot::snapshot(&api, block_hash).await?,
        Commands::System(system_commands) => match system_commands {
            SystemCommands::Account { who } => {
                let account = api.storage().system().account(&who, Some(block_hash)).await;
                println!("{:#?}", account);
            }
            SystemCommands::Events => {
                let events = api.storage().system().events(Some(block_hash)).await;
                println!("{:#?}", events);
            }
            SystemCommands::BlockHash => {
                const TWOX_HASH_LEN: usize = 16; // 8 bytes hex
                const STORAGE_PREFIX_LEN: usize = 64; // 32 bytes hex

                let mut iter = api
                    .storage()
                    .system()
                    .block_hash_iter(Some(block_hash))
                    .await?;

                while let Some((key, hash)) = iter.next().await? {
                    let key = hex::encode(&key.0);
                    let number: u32 = Decode::decode(
                        &mut hex::decode(&key[STORAGE_PREFIX_LEN + TWOX_HASH_LEN..])?.as_slice(),
                    )?;
                    println!("{}: {}", number, hash);
                }
            }
        },
    }

    Ok(())
}
