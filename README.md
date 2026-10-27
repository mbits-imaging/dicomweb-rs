# DICOMWEB-rs

[![continuous integration](https://github.com/mbits-imaging/dicomweb-rs/actions/workflows/rust.yml/badge.svg)](https://github.com/mbits-imaging/dicomweb-rs/actions/workflows/rust.yml)

This is a DICOMweb client library, using the excellent [dicom-rs](https://github.com/Enet4/dicom-rs) crate.

## Features

### QIDO-RS

| Method | Route                                   | Client function             | Description                              |
| ------ | --------------------------------------- | --------------------------- | ---------------------------------------- |
| GET    | `/studies`                              | `query_studies`             | Query all studies                        |
| GET    | `/studies/{uid}/series`                 | `query_series_in_study`     | Query all series in a specific study     |
| GET    | `/studies/{uid}/series/{uid}/instances` | `query_instances_in_series` | Query all instances in a specific series |
| GET    | `/series`                               | `query_series`              | Query all series                         |
| GET    | `/instances`                            | `query_instances`           | Query all instances                      |

### WADO-RS

| Method | Route                                                            | Client function              | Description                                       |
| ------ | ---------------------------------------------------------------- | ---------------------------- | ------------------------------------------------- |
| GET    | `/studies/{uid}`                                                 | `retrieve_study`             | Retrieve all instances in a specific study        |
| GET    | `/studies/{uid}/metadata`                                        | `retrieve_study_metadata`    | Retrieve metadata for all instances in a study    |
| GET    | `/studies/{uid}/series/{uid}`                                    | `retrieve_series`            | Retrieve all instances in a specific series       |
| GET    | `/studies/{uid}/series/{uid}/metadata`                           | `retrieve_series_metadata`   | Retrieve metadata for all instances in a series   |
| GET    | `/studies/{uid}/series/{uid}/instances/{uid}`                    | `retrieve_instance`          | Retrieve a single instance                        |
| GET    | `/studies/{uid}/series/{uid}/instances/{uid}/metadata`           | `retrieve_instance_metadata` | Retrieve metadata for a specific instance         |
| GET    | `/studies/{uid}/series/{uid}/instances/{uid}/frames/{framelist}` | `retrieve_frames`            | Retrieve frame pixel data for a specific instance |

### STOW-RS

| Method | Route            | Client function            | Description                         |
| ------ | ---------------- | -------------------------- | ----------------------------------- |
| POST   | `/studies`       | `store_instances`          | Store instances                     |
| POST   | `/studies/{uid}` | `store_instances_in_study` | Store instances in a specific study |

### MWL-RS

| Method | Route                                 | Client function                            | Description                 |
| ------ | ------------------------------------- | ------------------------------------------ | --------------------------- |
| GET    | `/modality-scheduled-procedure-steps` | `query_modality_scheduled_procedure_steps` | Query the modality worklist |

### MPPS-RS

| Method | Route                                              | Client function | Description                                  |
| ------ | -------------------------------------------------- | --------------- | -------------------------------------------- |
| POST   | `/modality-performed-procedure-steps/{uid}`        | `create_mpps`   | Create a modality performed procedure step   |
| POST   | `/modality-performed-procedure-steps/{uid}?update` | `update_mpps`   | Update a modality performed procedure step   |
| GET    | `/modality-performed-procedure-steps/{uid}`        | `retrieve_mpps` | Retrieve a modality performed procedure step |

### Storage Commitment

| Method | Route                        | Client function              | Description                                       |
| ------ | ---------------------------- | ---------------------------- | ------------------------------------------------- |
| POST   | `/commitment-requests/{uid}` | `request_storage_commitment` | Request storage commitment for a set of instances |
| GET    | `/commitment-requests/{uid}` | `check_storage_commitment`   | Check the result of a storage commitment request  |

### ASDO-RS

[Supplement 248](https://www.dicomstandard.org/News-dir/ftsup/docs/sups/sup248.pdf), not yet part of the standard.

| Method | Route                                                       | Client function            | Description                             |
| ------ | ----------------------------------------------------------- | -------------------------- | --------------------------------------- |
| POST   | `/studies/send-requests/{uid}`                              | `send_studies`             | Send all studies to a destination       |
| GET    | `/studies/send-requests/{uid}`                              | `send_studies_status`      | Check the status of a send request      |
| POST   | `/studies/{uid}/series/send-requests/{uid}`                 | `send_series_in_study`     | Send all series in a specific study     |
| POST   | `/studies/{uid}/instances/send-requests/{uid}`              | `send_instances_in_study`  | Send all instances in a specific study  |
| POST   | `/studies/{uid}/series/{uid}/instances/send-requests/{uid}` | `send_instances_in_series` | Send all instances in a specific series |

## Usage

```rust
use dicom_web::DicomWebClient;

let mut client = DicomWebClient::with_single_url("http://localhost:8042");
client.set_basic_auth("orthanc", "orthanc");

let studies = client.query_studies().run().await.unwrap();
```
