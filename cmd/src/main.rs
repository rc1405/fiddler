use clap::{Args, Parser};
use futures::stream::FuturesOrdered;
use futures::stream::StreamExt;
use inline_colorization::{color_green, color_red, color_reset};
use serde::Serialize;
use std::fs;
use std::process;
use tracing_subscriber::filter::{EnvFilter, LevelFilter};

use fiddler::Runtime;
use fiddler::Error;

mod test;

#[derive(Parser)]
#[command(name = "fiddler")]
#[command(bin_name = "fiddler")]
enum FiddlerCli {
    Lint(LintArgs),
    Run(RunArgs),
    Test(RunArgs),
}

#[derive(Args)]
#[command(author, version, about, long_about = None)]
struct LintArgs {
    #[arg(short, long)]
    config: Vec<String>,
}

#[derive(clap::ValueEnum, Clone, Default, Debug, Serialize)]
enum LogLevel {
    Info,
    Debug,
    Trace,
    Error,
    #[default]
    None,
}

#[derive(Args)]
#[command(author, version, about, long_about = None)]
struct RunArgs {
    #[arg(short, long)]
    config: Vec<String>,
    #[arg(short, long, value_enum, default_value = "none")]
    log_level: LogLevel,
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    match FiddlerCli::parse() {
        FiddlerCli::Lint(args) => {
            let mut failures: Vec<String> = Vec::new();
            for c in args.config {
                let conf = match fs::read_to_string(&c) {
                    Ok(f) => f,
                    Err(e) => {
                        failures.push(format!("failed {}: {}", c, e));
                        continue;
                    }
                };

                if let Err(e) = Runtime::from_config(&conf) {
                    failures.push(format!("failed {}: {}", c, e));
                    continue;
                };
            }

            if failures.is_empty() {
                println!("{color_green}Configuration is valid{color_reset}");
                process::exit(0)
            };

            for f in failures {
                println!("{color_red}{}{color_reset}", f);
            }

            process::exit(1);
        }
        FiddlerCli::Run(args) => {
            setup_subscriber(args.log_level);

            let mut environments = Vec::new();
            for c in args.config {
                let conf = fs::read_to_string(&c).map_err(|e| {
                    Error::ConfigurationItemNotFound(format!("cannot read {}: {}", c, e))
                })?;
                let env = Runtime::from_config(&conf)?;
                environments.push(env);
            }

            let new_futures =
                FuturesOrdered::from_iter(environments.iter().map(|e| e.run())).fuse();
            let future_to_await = new_futures.collect::<Vec<Result<(), Error>>>();
            futures::pin_mut!(future_to_await);
            let results = future_to_await.await;
            for r in results {
                r?
            }
            process::exit(0)
        }
        FiddlerCli::Test(args) => {
            setup_subscriber(args.log_level);
            test::handle_tests(args.config).await?;
            Ok(())
        }
    }
}

fn setup_subscriber(arg_log_level: LogLevel) {
    let log_level = match arg_log_level {
        LogLevel::Debug => Some(LevelFilter::DEBUG),
        LogLevel::Error => Some(LevelFilter::ERROR),
        LogLevel::Info => Some(LevelFilter::INFO),
        LogLevel::Trace => Some(LevelFilter::TRACE),
        LogLevel::None => None,
    };

    if let Some(l) = log_level {
        let filter = EnvFilter::builder()
            .with_default_directive(LevelFilter::OFF.into())
            .from_env()
            .unwrap()
            .add_directive(format!("fiddler::runtime={}", l).parse().unwrap());

        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .compact()
            .json()
            .init();
    };
}
