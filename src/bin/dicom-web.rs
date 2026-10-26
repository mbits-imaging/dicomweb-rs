//! A CLI tool for performing DICOMweb operations
use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};
use dicom_core::Tag;
use dicom_dump::DumpOptions;
use dicom_json::DicomJson;
use dicom_object::InMemDicomObject;
use dicom_web::{CommitmentStatus, DicomWebClient};
use tracing::{Level, error};

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

#[derive(ValueEnum, Clone, Debug)]
enum MppsAction {
    Create,
    Update,
    Get,
}

/// Parse a tag in the DICOMweb radix form (e.g. 00400252)
fn parse_tag(value: &str) -> Result<Tag, String> {
    if value.len() != 8 {
        return Err(format!("Invalid tag {value}, expected 8 hex digits"));
    }
    let tag = u32::from_str_radix(value, 16).map_err(|e| format!("Invalid tag {value}: {e}"))?;
    Ok(Tag((tag >> 16) as u16, tag as u16))
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
    /// Perform an MPPS-RS operation.
    Mpps {
        /// The MPPS operation to perform
        action: MppsAction,

        /// The MPPS UID (SOP Instance UID of the Modality Performed Procedure Step)
        #[clap(long = "uid")]
        mpps_uid: String,

        /// Path to a DICOM JSON file with the data set to send.
        /// Required for create and update.
        #[clap(long = "dataset")]
        dataset: Option<PathBuf>,

        /// The tags to include in the response of get, e.g. 00400252
        #[clap(long = "includefield", value_parser = parse_tag)]
        includefields: Vec<Tag>,
    },
    /// Perform a Storage Commitment operation.
    Stgcmt {
        /// The transaction UID of the storage commitment request.
        /// If not provided, a random UID will be generated.
        #[clap(long = "transaction")]
        transaction_uid: Option<String>,

        /// The instances to commit. The SOP Class UID and SOP Instance UID are read from the files.
        #[clap(long = "instances")]
        instances: Vec<PathBuf>,

        /// Only check the result of an earlier request once, without committing.
        #[clap(long = "check", default_value_t = false)]
        check: bool,

        /// Seconds to wait between checking the result of a pending request
        #[clap(long = "poll-interval", default_value_t = 1)]
        poll_interval: u64,

        /// Seconds to wait for the result of a pending request
        #[clap(long = "timeout", default_value_t = 60)]
        timeout: u64,
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
                        eprintln!(
                            "Error: Destination password must be provided when destination username is set"
                        );
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
        Mode::Mpps {
            action,
            mpps_uid,
            dataset,
            includefields,
        } => {
            println!("MPPS-RS mode");
            match action {
                MppsAction::Create | MppsAction::Update => {
                    let path = dataset.unwrap_or_else(|| {
                        eprintln!("Error: --dataset must be provided for create and update");
                        std::process::exit(ERROR_OTHER);
                    });
                    let json = std::fs::read_to_string(path).unwrap_or_else(|e| {
                        eprintln!("Error: {:?}", e);
                        std::process::exit(ERROR_READ);
                    });
                    let dataset = serde_json::from_str::<DicomJson<InMemDicomObject>>(&json)
                        .unwrap_or_else(|e| {
                            eprintln!("Error: {:?}", e);
                            std::process::exit(ERROR_READ);
                        })
                        .into_inner();

                    let builder = match action {
                        MppsAction::Create => client.create_mpps(&mpps_uid),
                        _ => client.update_mpps(&mpps_uid),
                    };

                    builder
                        .with_dataset(dataset)
                        .run()
                        .await
                        .unwrap_or_else(|e| {
                            eprintln!("Error: {:?}", e);
                            std::process::exit(ERROR_OTHER);
                        });
                    println!("MPPS {} succeeded", mpps_uid);
                }
                MppsAction::Get => {
                    let mpps = client
                        .retrieve_mpps(&mpps_uid)
                        .with_includefields(includefields)
                        .run()
                        .await
                        .unwrap_or_else(|e| {
                            eprintln!("Error: {:?}", e);
                            std::process::exit(ERROR_OTHER);
                        });

                    DumpOptions::new().dump_object(&mpps).unwrap_or_else(|e| {
                        eprintln!("Error: {:?}", e);
                        std::process::exit(ERROR_DUMP);
                    });
                }
            }
        }
        Mode::Stgcmt {
            transaction_uid,
            instances,
            check,
            poll_interval,
            timeout,
        } => {
            println!("Storage Commitment mode");
            let status = if check {
                let transaction_uid = transaction_uid.unwrap_or_else(|| {
                    eprintln!("Error: Transaction UID must be provided when --check is set");
                    std::process::exit(ERROR_OTHER);
                });

                client
                    .check_storage_commitment(&transaction_uid)
                    .run()
                    .await
                    .unwrap_or_else(|e| {
                        eprintln!("Error: {:?}", e);
                        std::process::exit(ERROR_OTHER);
                    })
            } else {
                let transaction_uid = transaction_uid
                    .unwrap_or_else(|| format!("2.25.{}", uuid::Uuid::new_v4().as_u128()));
                println!("Transaction UID: {}", transaction_uid);

                let mut builder = client.request_storage_commitment(&transaction_uid);
                for path in instances {
                    let instance = dicom_object::open_file(path).unwrap_or_else(|e| {
                        eprintln!("Error: {:?}", e);
                        std::process::exit(ERROR_READ);
                    });
                    builder = builder.with_instance(
                        instance.meta().media_storage_sop_class_uid(),
                        instance.meta().media_storage_sop_instance_uid(),
                    );
                }

                let mut status = builder.run().await.unwrap_or_else(|e| {
                    eprintln!("Error: {:?}", e);
                    std::process::exit(ERROR_OTHER);
                });

                // Poll for the result until the request is processed or the timeout expires
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout);
                while matches!(status, CommitmentStatus::Pending)
                    && std::time::Instant::now() < deadline
                {
                    tokio::time::sleep(std::time::Duration::from_secs(poll_interval)).await;
                    status = client
                        .check_storage_commitment(&transaction_uid)
                        .run()
                        .await
                        .unwrap_or_else(|e| {
                            eprintln!("Error: {:?}", e);
                            std::process::exit(ERROR_OTHER);
                        });
                }
                status
            };

            match status {
                CommitmentStatus::Pending => println!("Storage commitment is still pending"),
                CommitmentStatus::Done(result) => {
                    DumpOptions::new().dump_object(&result).unwrap_or_else(|e| {
                        eprintln!("Error: {:?}", e);
                        std::process::exit(ERROR_DUMP);
                    });
                }
            }
        }
    }
}
