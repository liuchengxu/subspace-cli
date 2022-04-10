use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use subxt::{
    sp_core::{
        crypto::{AccountId32 as AccountId, Ss58Codec},
        sr25519, Pair, H256,
    },
    sp_runtime::traits::Header as HeaderT,
    ClientBuilder, DefaultConfig, SubstrateExtrinsicParams,
};

const BLAKE_HASH_LEN: usize = 32; // 16 bytes hex
const STORAGE_PREFIX_LEN: usize = 64; // 32 bytes hex

/// List of accounts which should receive token grants.
const TOKEN_GRANTS: &[&str] = &[
    "5Dns1SVEeDqnbSm2fVUqHJPCvQFXHVsgiw28uMBwmuaoKFYi",
    "5DxtHHQL9JGapWCQARYUAWj4yDcwuhg9Hsk5AjhEzuzonVyE",
    "5EHhw9xuQNdwieUkNoucq2YcateoMVJQdN8EZtmRy3roQkVK",
    "5C5qYYCQBnanGNPGwgmv6jiR2MxNPrGnWYLPFEyV1Xdy2P3x",
    "5GBWVfJ253YWVPHzWDTos1nzYZpa9TemP7FpQT9RnxaFN6Sz",
    "5F9tEPid88uAuGbjpyegwkrGdkXXtaQ9sGSWEnYrfVCUCsen",
    "5DkJFCv3cTBsH5y1eFT94DXMxQ3EmVzYojEA88o56mmTKnMp",
    "5G23o1yxWgVNQJuL4Y9UaCftAFvLuMPCRe7BCARxCohjoHc9",
    "5GhHwuJoK1b7uUg5oi8qUXxWHdfgzv6P5CQSdJ3ffrnPRgKM",
    "5EqBwtqrCV427xCtTsxnb9X2Qay39pYmKNk9wD9Kd62jLS97",
    "5D9pNnGCiZ9UqhBQn5n71WFVaRLvZ7znsMvcZ7PHno4zsiYa",
    "5DXfPcXUcP4BG8LBSkJDrfFNApxjWySR6ARfgh3v27hdYr5S",
    "5CXSdDJgzRTj54f9raHN2Z5BNPSMa2ETjqCTUmpaw3ECmwm4",
    "5DqKxL7bQregQmUfFgzTMfRKY4DSvA1KgHuurZWYmxYSCmjY",
    "5CfixiS93yTwHQbzzfn8P2tMxhKXdTx7Jam9htsD7XtiMFtn",
    "5FZe9YzXeEXe7sK5xLR8yCmbU8bPJDTZpNpNbToKvSJBUiEo",
    "5FZwEgsvZz1vpeH7UsskmNmTpbfXvAcojjgVfShgbRqgC1nx",
];

#[subxt::subxt(runtime_metadata_path = "subspace_metadata.scale")]
mod subspace {}

type Balance = u128;
type BlockHash = H256;
type BlockNumber = u32;

type SubspaceRuntimeApi =
    subspace::RuntimeApi<DefaultConfig, SubstrateExtrinsicParams<DefaultConfig>>;

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
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = SubspaceCli::parse();

    let api = ClientBuilder::new()
        .set_url(cli.url)
        .build()
        .await?
        .to_runtime_api::<SubspaceRuntimeApi>();

    let maybe_block_hash = if let Some(block_number) = cli.block_number {
        Some(
            api.client
                .rpc()
                .block_hash(Some(block_number.into()))
                .await?
                .expect(&format!(
                    "Block hash for block number {} not found",
                    block_number
                )),
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
        Commands::Snapshot => snapshot(&api, block_hash).await?,
        Commands::System(system_commands) => match system_commands {
            SystemCommands::Account { who } => {
                let account = api.storage().system().account(&who, Some(block_hash)).await;
                println!("{:#?}", account);
            }
        },
    }

    Ok(())
}

async fn snapshot(api: &SubspaceRuntimeApi, block_hash: BlockHash) -> Result<()> {
    let endowed = vec![
        AccountId::from_ss58check("5CXTmJEusve5ixyJufqHThmy4qUrrm6FyLCR7QfE4bbyMTNC")
            .expect("Sudo account must be valid; qed"),
        sr25519::Pair::from_string("//Alice", None)
            .expect("Could not generate a key pair")
            .public()
            .into(),
        sr25519::Pair::from_string("//Bob", None)
            .expect("Could not generate a key pair")
            .public()
            .into(),
    ];

    let token_grants = TOKEN_GRANTS
        .iter()
        .filter_map(|address| AccountId::from_ss58check(address).ok())
        .collect::<Vec<_>>();

    assert_eq!(token_grants.len(), TOKEN_GRANTS.len());

    let mut new_accounts = Vec::new();

    let mut iter = api
        .storage()
        .system()
        .account_iter(Some(block_hash))
        .await?;

    let mut total_issuance = 0;

    while let Some((key, account)) = iter.next().await? {
        let pubkey = &hex::encode(&key.0)[STORAGE_PREFIX_LEN + BLAKE_HASH_LEN..];
        let account_id = pubkey
            .parse::<AccountId>()
            .map_err(|err| anyhow!("{}", err))?;

        let total = account.data.free + account.data.reserved;

        total_issuance += total;

        if token_grants.contains(&account_id) || endowed.contains(&account_id) {
            // Vesting and endowed accounts are ignored.
            continue;
        } else {
            // New accounts must have the free balance only.
            assert_eq!(total, account.data.free);
            new_accounts.push((account_id, total));
        }
    }

    let expected_total_issuance = api
        .storage()
        .balances()
        .total_issuance(Some(block_hash))
        .await?;

    assert_eq!(total_issuance, expected_total_issuance);

    let block_header = api
        .client
        .rpc()
        .header(Some(block_hash.into()))
        .await?
        .expect(&format!("Header for block hash {} not found", block_hash));

    println!(
        "State of balances at block #{:?} ({:?})",
        block_header.number(),
        block_hash
    );
    println!("Total new accounts: {}", new_accounts.len());
    println!(
        "Total new issuance: {}",
        new_accounts
            .iter()
            .map(|(_, balance)| balance)
            .sum::<Balance>()
    );

    let mut path = std::env::current_dir()?;
    path.push(format!("balances_{}.json", block_header.number()));

    let file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .open(&path)?;

    serde_json::to_writer_pretty(&file, &new_accounts)?;

    println!(
        "Snapshot has been successfully written to {}",
        path.display()
    );

    Ok(())
}
