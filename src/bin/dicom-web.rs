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
    Stow {
        /// The study instance UID
        /// If not provided, store without any checks
        #[clap(long = "study")]
        study_uid: Option<String>,

        /// The instances paths
        #[clap(long = "instances")]
        instances: Vec<PathBuf>,
    },
    /// Perform a MWL-RS operation.
    Mwl {},
    /// Perform an ASDO-RS operation.
    Asdo {
        /// The transaction UID for the ASDO-RS request.
        /// If not provided, a random UUID will be generated.
        #[clap(long = "transaction")]
        transaction_uid: Option<String>,

        /// The destination for the ASDO-RS request. Will be passed as a query parameter.
        #[clap(long = "destination")]
        destination: String,

        #[clap(long = "dst-username")]
        dst_username: Option<String>,
        #[clap(long = "dst-password")]
        dst_password: Option<String>,
        #[clap(long = "dst-token")]
        dst_token: Option<String>,

        #[clap(long = "status", default_value_t = false)]
        status: bool,

        /// The level of the query
        level: DicomLevel,
    },
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

    /// The bearer token for authentication
    /// If not provided, no authentication is used
    #[clap(long = "token")]
    token: Option<String>,

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

    if let Some(token) = app.token {
        client.set_bearer_token(&token);
    }

    match app.mode {
        Mode::Qido {
            fuzzy_matching,
            level,
        } => {
            println!("QIDO-RS mode");
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
            println!("Received {} DICOM objects", results.len());
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
        Mode::Stow {
            study_uid,
            instances,
        } => {
            println!("STOW-RS mode");
            let builder = match study_uid {
                Some(study_uid) => client.store_instances_in_study(&study_uid),
                None => client.store_instances(),
            };

            // Open all instances as DICOM
            let instances: Vec<_> = instances
                .into_iter()
                .map(|path| {
                    dicom_object::open_file(path).unwrap_or_else(|e| {
                        eprintln!("Error: {:?}", e);
                        std::process::exit(ERROR_READ);
                    })
                })
                .collect();

            // Create a stream of instances
            let instance_stream = futures_util::stream::iter(instances);

            // Store the instances
            let stored_instances = builder
                .with_instances(instance_stream)
                .run()
                .await
                .unwrap_or_else(|e| {
                    eprintln!("Error: {:?}", e);
                    std::process::exit(ERROR_OTHER);
                });
            println!("Stored DICOM objects");

            DumpOptions::new()
                .dump_object(&stored_instances)
                .unwrap_or_else(|e| {
                    eprintln!("Error: {:?}", e);
                    std::process::exit(ERROR_DUMP);
                });
        }
        Mode::Mwl {} => {
            println!("MWL-RS mode");
        }
        Mode::Asdo {
            transaction_uid,
            destination,
            dst_username,
            dst_password,
            dst_token,
            status,
            level,
        } => {
            println!("ASDO-RS mode");
            // Only check the status for the transaction
            let status = if status {
                let transaction_uid = transaction_uid.unwrap_or_else(|| {
                    eprintln!("Error: Transaction UID must be provided when --status is set");
                    std::process::exit(ERROR_OTHER);
                });

                let builder = match level {
                    DicomLevel::Study => client.send_studies_status(&transaction_uid),
                    _ => unimplemented!("Only study level is currently supported for ASDO-RS"),
                };

                builder.run().await.unwrap_or_else(|e| {
                    eprintln!("Error: {:?}", e);
                    std::process::exit(ERROR_OTHER);
                })
            } else {
                let transaction_uid =
                    transaction_uid.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

                let mut builder = match level {
                    DicomLevel::Study => client.send_studies(&transaction_uid),
                    _ => unimplemented!("Only study level is currently supported for ASDO-RS"),
                };

                if let Some(username) = dst_username {
                    if let Some(password) = dst_password {
                        builder = builder.with_basic_auth(username, password);
                    } else {
                        eprintln!("Error: Destination password must be provided when destination username is set");
                        std::process::exit(ERROR_OTHER);
                    }
                } else if let Some(token) = dst_token {
                    builder = builder.with_bearer_token(token);
                }

                builder
                    .with_destination(destination)
                    .run()
                    .await
                    .unwrap_or_else(|e| {
                        eprintln!("Error: {:?}", e);
                        std::process::exit(ERROR_OTHER);
                    })
            };

            DumpOptions::new().dump_object(&status).unwrap_or_else(|e| {
                eprintln!("Error: {:?}", e);
                std::process::exit(ERROR_DUMP);
            });
        }
    }
}
