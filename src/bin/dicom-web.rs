//! A CLI tool for performing DICOMweb operations
use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};
use dicom_dump::DumpOptions;
use dicom_web::DicomWebClient;
use tracing::{error, Level};

/// Exit code for when an error emerged while reading the DICOM file.
const ERROR_READ: i32 = -2;
/// Exit code for when an error emerged while transcoding the file.
const ERROR_TRANSCODE: i32 = -3;
/// Exit code for when an error emerged while writing the file.
const ERROR_WRITE: i32 = -4;
/// Exit code for when an error emerged while dumping the output.
const ERROR_DUMP: i32 = -5;
/// Exit code for when an error emerged while writing the file.
const ERROR_OTHER: i32 = -128;

#[derive(ValueEnum, Clone, Default, Debug)]
enum DicomLevel {
    #[default]
    Study,
    Series,
    Instance,
}

#[derive(Debug, Subcommand)]
enum Mode {
    /// Perform a QIDO-RS operation.
    Qido {
        /// Wether or not to use fuzzy matching
        #[clap(long, short = 'f')]
        fuzzy_matching: bool,

        /// The level of the query
        level: DicomLevel,
    },
    /// Perform a WADO-RS operation.
    Wado {},
    /// Perform a STOW-RS operation.
    Stow {},
    /// Perform a MWL-RS operation.
    Mwl {},
}

/// Transcode a DICOM file
#[derive(Debug, Parser)]
#[command(version)]
struct App {
    /// The output to save retrieved data
    #[clap(short = 'o', long = "output")]
    output: Option<PathBuf>,

    #[clap(subcommand)]
    mode: Mode,

    /// The URL of the DICOMweb server
    #[clap(short = 'u', long = "url")]
    url: String,

    /// The username for authentication
    /// If not provided, no authentication is used
    #[clap(long = "username")]
    username: Option<String>,

    /// The password for authentication
    /// If not provided, no authentication is used
    #[clap(long = "password")]
    password: Option<String>,

    /// Verbose mode
    #[clap(short = 'v', long = "verbose")]
    verbose: bool,
}

#[tokio::main]
async fn main() {
    let app = App::parse();

    tracing::subscriber::set_global_default(
        tracing_subscriber::FmtSubscriber::builder()
            .with_max_level(if app.verbose {
                Level::DEBUG
            } else {
                Level::INFO
            })
            .finish(),
    )
    .unwrap_or_else(|e| {
        error!("{}", snafu::Report::from_error(e));
    });

    let mut client = DicomWebClient::with_single_url(&app.url);

    if let (Some(username), Some(password)) = (app.username, app.password) {
        client.set_basic_auth(&username, &password);
    }

    match app.mode {
        Mode::Qido {
            fuzzy_matching,
            level,
        } => {
            let mut builder = match level {
                DicomLevel::Study => client.query_studies(),
                DicomLevel::Series => client.query_series(),
                DicomLevel::Instance => client.query_instances(),
            };

            let results = builder
                .with_fuzzymatching(fuzzy_matching)
                .run()
                .await
                .unwrap_or_else(|e| {
                    eprintln!("Error: {:?}", e);
                    std::process::exit(ERROR_OTHER);
                });

            let mut i = 0;
            for dcm in results {
                println!("DICOM object #{}", i);
                DumpOptions::new().dump_object(&dcm).unwrap_or_else(|e| {
                    eprintln!("Error: {:?}", e);
                    std::process::exit(ERROR_DUMP);
                });
                i += 1;
            }
        }
        Mode::Wado {} => {
            println!("WADO-RS mode");
        }
        Mode::Stow {} => {
            println!("STOW-RS mode");
        }
        Mode::Mwl {} => {
            println!("MWL-RS mode");
        }
    }
}
