// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Tests for the `asvo` module.
//!
//! Grouped into submodules by the source file they were originally colocated
//! with: `asvo_serde_tests` (from `asvo_serde.rs`), `types_tests` (from
//! `types.rs`), and `client_tests` (from `mod.rs`).

/// Tests moved from `asvo_serde.rs`: parsing of the JSON formats returned by
/// the ASVO.
mod asvo_serde_tests {
    use crate::asvo::asvo_serde::{parse_asvo_json, AsvoSubmitJobResponse};

    #[test]
    fn test_json_job_listing_parse() {
        let json = "[{\"action\": \"INSERT\", \"table\": \"jobs\", \"row\": {\"job_type\": 1, \"job_state\": \"completed\", \"user_id\": 1065, \"job_params\": {\"delivery\": \"acacia\", \"download_type\": \"vis\", \"job_type\": \"download\", \"obs_id\": \"1339896408\", \"priority\": 1, \"user_pawsey_group\": \"mwaops\"}, \"error_code\": null, \"error_text\": null, \"created\": \"2022-06-22T01:56:38.635146\", \"started\": \"2022-06-22T01:57:09.093927\", \"completed\": \"2022-06-22T01:57:24.693448\", \"product\": {\"files\": [{\"type\": \"acacia\", \"url\": \"https://ingest.pawsey.org.au/mwa-asvo/1339896408_575929_vis.tar?AWSAccessKeyId=0f61c75cd1184e5abc76500d71758927&Signature=XwoaCna8vNmMEBXcFji2boZ5yjk%3D&Expires=1656467844\", \"size\": 931112960, \"sha1\": \"12b0933ff3985c82a7303d8e57fa7157fe88353e\"}]}, \"id\": 575929}}]";
        let result = parse_asvo_json(json);
        assert!(
            result.is_ok(),
            "result is not ok: {:?}",
            result.err().unwrap()
        );
        let jobs = result.unwrap();
        assert_eq!(jobs.0.len(), 1);
        assert_eq!(jobs.0[0].jobid, 575929);
    }

    #[test]
    fn test_json_job_submit_response_parse() {
        let json = "{\"job_id\": 308874}";
        let decoded = serde_json::from_str::<AsvoSubmitJobResponse>(json);
        assert!(decoded.is_ok());
        assert_eq!(
            AsvoSubmitJobResponse::JobID { job_id: 308874 },
            decoded.unwrap()
        );
    }

    #[test]
    fn test_json_job_submit_response_bad_parse() {
        let json = "{\"error_code\": 0, \"error\": \"Download Type: Expected not None\"}";
        let decoded = serde_json::from_str::<AsvoSubmitJobResponse>(json);
        assert!(decoded.is_ok());
        assert_eq!(
            AsvoSubmitJobResponse::ErrorWithCode {
                error_code: 0,
                error: "Download Type: Expected not None".to_string(),
            },
            decoded.unwrap()
        );
    }

    #[test]
    fn test_json_job_submit_response_bad_parse2() {
        let json = "{\"error\": \"Permission denied\"}";
        let decoded = serde_json::from_str::<AsvoSubmitJobResponse>(json);
        assert!(decoded.is_ok());
        assert_eq!(
            AsvoSubmitJobResponse::GenericError {
                error: "Permission denied".to_string(),
            },
            decoded.unwrap()
        );
    }

    #[test]
    fn test_json_job_submit_response_job_already_q_p_c_parse() {
        let json = "{\"error\": \"Job already queued, processing or complete\", \"error_code\": 2, \"job_id\": 10001822}";
        let decoded = serde_json::from_str::<AsvoSubmitJobResponse>(json);
        assert!(decoded.is_ok());
        assert_eq!(
            AsvoSubmitJobResponse::JobIDWithError {
                error_code: 2,
                error: "Job already queued, processing or complete".to_string(),
                job_id: 10001822
            },
            decoded.unwrap()
        );
    }

    #[test]
    fn test_json_job_submit_response_job_already_q_p_parse() {
        let json = "{\"error\": \"Job already queued or processing.\", \"error_code\": 2, \"job_id\": 10001822}";
        let decoded = serde_json::from_str::<AsvoSubmitJobResponse>(json);
        assert!(decoded.is_ok());
        assert_eq!(
            AsvoSubmitJobResponse::JobIDWithError {
                error_code: 2,
                error: "Job already queued or processing.".to_string(),
                job_id: 10001822
            },
            decoded.unwrap()
        );
    }

    #[test]
    fn test_json_job_submit_response_full_or_partial_outage1() {
        let json = "{\"error\": \"Your job cannot be submitted as there is a full outage in progress.\", \"error_code\": 0}";
        let decoded = serde_json::from_str::<AsvoSubmitJobResponse>(json);
        assert!(decoded.is_ok());
        assert_eq!(
            AsvoSubmitJobResponse::ErrorWithCode {
                error_code: 0,
                error: "Your job cannot be submitted as there is a full outage in progress."
                    .to_string(),
            },
            decoded.unwrap()
        );
    }

    #[test]
    fn test_json_job_submit_response_full_or_partial_outage2() {
        let json = "{\"error\": \"Your job cannot be submitted as there is a partial outage, please use a delivery location other than acacia.\", \"error_code\": 0}";
        let decoded = serde_json::from_str::<AsvoSubmitJobResponse>(json);
        assert!(decoded.is_ok());
        assert_eq!(
            AsvoSubmitJobResponse::ErrorWithCode {
                error_code: 0,
                error: "Your job cannot be submitted as there is a partial outage, please use a delivery location other than acacia."
                    .to_string(),
            },
            decoded.unwrap()
        );
    }

    #[test]
    fn test_json_job_submit_response_full_or_partial_outage3() {
        let json = "{\"error\": \"Your job cannot be submitted as there is a partial outage, please use a delivery location other than scratch.\", \"error_code\": 0}";
        let decoded = serde_json::from_str::<AsvoSubmitJobResponse>(json);
        assert!(decoded.is_ok());
        assert_eq!(
            AsvoSubmitJobResponse::ErrorWithCode {
                error_code: 0,
                error: "Your job cannot be submitted as there is a partial outage, please use a delivery location other than scratch."
                    .to_string(),
            },
            decoded.unwrap()
        );
    }

    #[test]
    fn test_json_job_submit_response_full_or_partial_outage4() {
        let json = "{\"error\": \"Your job cannot be submitted as there is a partial outage, please use a delivery location other than dug.\", \"error_code\": 0}";
        let decoded = serde_json::from_str::<AsvoSubmitJobResponse>(json);
        assert!(decoded.is_ok());
        assert_eq!(
            AsvoSubmitJobResponse::ErrorWithCode {
                error_code: 0,
                error: "Your job cannot be submitted as there is a partial outage, please use a delivery location other than dug."
                    .to_string(),
            },
            decoded.unwrap()
        );
    }

    #[test]
    fn test_json_job_submit_response_full_or_partial_outage5() {
        let json = "{\"error\": \"Your job cannot be submitted as the staging server is down and also acacia is unavailable!\", \"error_code\": 0}";
        let decoded = serde_json::from_str::<AsvoSubmitJobResponse>(json);
        assert!(decoded.is_ok());
        assert_eq!(
            AsvoSubmitJobResponse::ErrorWithCode {
                error_code: 0,
                error: "Your job cannot be submitted as the staging server is down and also acacia is unavailable!"
                    .to_string(),
            },
            decoded.unwrap()
        );
    }
}

/// Tests moved from `types.rs`: parsing/validation of ASVO data types.
mod types_tests {
    use std::str::FromStr;

    use crate::{AsvoError, AsvoJobState, AsvoJobType, Delivery};

    #[test]
    fn test_asvo_job_state_fromstr() {
        assert!(matches!(
            AsvoJobState::from_str("__Q_u_e_u_e_D__"),
            Ok(AsvoJobState::Queued)
        ));
        assert!(matches!(
            AsvoJobState::from_str("invalid job state"),
            Err(AsvoError::InvalidJobState { .. })
        ));
    }

    #[test]
    fn test_asvo_job_type_fromstr() {
        assert!(matches!(
            AsvoJobType::from_str("DownloadVisibilities"),
            Ok(AsvoJobType::DownloadVisibilities)
        ));
        assert!(matches!(
            AsvoJobType::from_str("download_visibilities"),
            Ok(AsvoJobType::DownloadVisibilities)
        ));
        assert!(matches!(
            AsvoJobType::from_str("download_voltages"),
            Ok(AsvoJobType::DownloadVoltage)
        ));
        assert!(matches!(
            AsvoJobType::from_str("download_beamformer"),
            Ok(AsvoJobType::DownloadBeamformer)
        ));

        assert!(matches!(
            AsvoJobType::from_str("imaging"),
            Ok(AsvoJobType::Imaging)
        ));

        assert!(matches!(
            AsvoJobType::from_str("Some unknown job type"),
            Ok(AsvoJobType::Unknown)
        ));
    }

    #[test]
    fn test_delivery_type_fromstr() {
        assert!(matches!(
            Delivery::validate(Some("acacia")),
            Ok(Delivery::Acacia)
        ));
        assert!(matches!(
            Delivery::validate(Some("ACACIA")),
            Err(AsvoError::InvalidDelivery { .. })
        ));
        assert!(matches!(
            Delivery::validate(Some("Acacia")),
            Err(AsvoError::InvalidDelivery { .. })
        ));
        assert!(matches!(Delivery::validate(Some("dug")), Ok(Delivery::Dug)));
        assert!(matches!(
            Delivery::validate(Some("scratch")),
            Ok(Delivery::Scratch)
        ));
        assert!(matches!(
            Delivery::validate(Some("invalid delivery type")),
            Err(AsvoError::InvalidDelivery { .. })
        ));
    }
}

/// Tests moved from `mod.rs`: live `AsvoClient` integration tests. These hit
/// the real MWA ASVO server and require `MWA_ASVO_API_KEY` to be set.
mod client_tests {
    use std::collections::BTreeMap;
    use std::thread;
    use std::time::Duration;

    use rand::seq::IteratorRandom;

    use crate::AsvoError;
    use crate::Delivery;
    use crate::DeliveryFormat;
    use crate::{AsvoClient, Obsid};

    #[test]
    fn test_create_asvo_client() {
        let client = AsvoClient::new();
        assert!(client.is_ok());
    }

    #[test]
    fn test_get_jobs() {
        let client = AsvoClient::new();
        let jobs = client.unwrap().get_jobs();
        assert!(jobs.is_ok());
    }

    #[test]
    fn test_submit_vis() {
        let client = AsvoClient::new().unwrap();
        let obs_id = Obsid::validate(1343457784).unwrap();
        let delivery = Delivery::Acacia;
        let delivery_format: Option<DeliveryFormat> = None;
        let allow_resubmit: bool = false;

        let vis_job = client.submit_vis(obs_id, delivery, delivery_format, allow_resubmit);
        match vis_job {
            Ok(_) => (),
            Err(error) => match error {
                AsvoError::BadStatus { code, message } => {
                    println!("Got return code {} with message {}", code, message)
                }
                _ => {
                    if error.to_string().contains(
                        "Your job cannot be submitted as there is a full outage in progress",
                    ) {
                        println!("Expected error occurred: {}", error);
                    } else {
                        panic!("Unexpected error has occured {}.", error);
                    }
                }
            },
        }
    }

    #[test]
    fn test_submit_conv() {
        let client = AsvoClient::new().unwrap();
        let obs_id = Obsid::validate(1343457784).unwrap();
        let delivery = Delivery::Acacia;
        let delivery_format: Option<DeliveryFormat> = None;
        let job_params = BTreeMap::new();
        let allow_resubmit: bool = false;

        let conv_job = client.submit_conv(
            obs_id,
            delivery,
            delivery_format,
            &job_params,
            allow_resubmit,
        );
        match conv_job {
            Ok(_) => (),
            Err(error) => match error {
                AsvoError::BadStatus { code, message } => {
                    println!("Got return code {} with message: {}", code, message)
                }
                _ => {
                    if error.to_string().contains(
                        "Your job cannot be submitted as there is a full outage in progress",
                    ) {
                        println!("Expected error occurred: {}", error);
                    } else {
                        panic!("Unexpected error has occured {}.", error);
                    }
                }
            },
        }
    }

    #[test]
    fn test_submit_meta() {
        let client = AsvoClient::new().unwrap();
        let obs_id = Obsid::validate(1343457784).unwrap();
        let delivery = Delivery::Acacia;
        let delivery_format: Option<DeliveryFormat> = None;
        let allow_resubmit: bool = false;

        let meta_job = client.submit_meta(obs_id, delivery, delivery_format, allow_resubmit);
        match meta_job {
            Ok(_) => (),
            Err(error) => match error {
                AsvoError::BadStatus { code, message } => {
                    println!("Got return code {} with message: {}", code, message)
                }
                _ => {
                    if error.to_string().contains(
                        "Your job cannot be submitted as there is a full outage in progress",
                    ) {
                        println!("Expected error occurred: {}", error);
                    } else {
                        panic!("Unexpected error has occured {}.", error);
                    }
                }
            },
        }
    }

    #[test]
    fn test_cancel_job_not_found() {
        let job_id = 0;
        let client = AsvoClient::new().unwrap();
        let cancel_result = client.cancel_asvo_job(job_id);
        let new_jobid_or_none = cancel_result.unwrap();
        assert!(new_jobid_or_none.is_none(), "{:?}", new_jobid_or_none);
    }

    #[test]
    fn test_cancel_job_successful() {
        let client = AsvoClient::new().unwrap();

        // submit a new job (don't worry we will cancel it right away)
        //
        // Due to potentially multiple test runs happening we need to randomise
        // the job params a bit so we don't have a situation where the job submission
        // fails because there is already an identical job running!
        #[derive(Clone)]
        struct Params<'a> {
            obs_id: Obsid,
            delivery: Delivery,
            delivery_format: Option<DeliveryFormat>,
            job_params: &'a BTreeMap<&'a str, &'a str>,
        }

        // Populate the choices

        // Averaging options
        let mut birli_10_0_5 = BTreeMap::new();
        birli_10_0_5.insert("avg_freq_res", "10");
        birli_10_0_5.insert("avg_time_res", "0.5");
        birli_10_0_5.insert("flag_edge_width", "80");
        let mut birli_20_1 = BTreeMap::new();
        birli_20_1.insert("avg_freq_res", "20");
        birli_20_1.insert("avg_time_res", "1");
        birli_20_1.insert("flag_edge_width", "80");
        let mut birli_40_1 = BTreeMap::new();
        birli_40_1.insert("avg_freq_res", "40");
        birli_40_1.insert("avg_time_res", "1");
        birli_40_1.insert("flag_edge_width", "80");
        let mut birli_40_2 = BTreeMap::new();
        birli_40_2.insert("avg_freq_res", "40");
        birli_40_2.insert("avg_time_res", "2");
        birli_40_2.insert("flag_edge_width", "80");
        let mut birli_80_2 = BTreeMap::new();
        birli_80_2.insert("avg_freq_res", "80");
        birli_80_2.insert("avg_time_res", "2");
        birli_80_2.insert("flag_edge_width", "80");

        let birli_options = [birli_10_0_5, birli_20_1, birli_40_1, birli_40_2, birli_80_2];

        let obs_list = [
            Obsid::validate(1416257384).unwrap(),
            Obsid::validate(1416257328).unwrap(),
            Obsid::validate(1416257272).unwrap(),
            Obsid::validate(1416257216).unwrap(),
            Obsid::validate(1416257160).unwrap(),
        ];

        let mut param_choices: Vec<Params> = Vec::new();

        for o in obs_list.iter() {
            for b in birli_options.iter() {
                param_choices.push(Params {
                    obs_id: *o,
                    delivery: Delivery::Acacia,
                    delivery_format: None,
                    job_params: b,
                });
            }
        }

        let mut new_job_id: Option<u32> = None;
        let mut attempt = 0;
        while new_job_id.is_none() && attempt < 5 {
            // Pick random set of params
            let p = &param_choices
                .clone()
                .into_iter()
                .choose(&mut rand::rng())
                .unwrap();

            let job_to_cancel =
                client.submit_conv(p.obs_id, p.delivery, p.delivery_format, p.job_params, true);

            match job_to_cancel {
                Ok(job_id_or_none) => {
                    if let Some(j) = job_id_or_none {
                        new_job_id = Some(j)
                    }
                }
                Err(error) => match error {
                    AsvoError::BadStatus { code, message } => {
                        println!("Got return code {} with message: {}", code, message)
                    }
                    _ => {
                        if error.to_string().contains(
                            "Your job cannot be submitted as there is a full outage in progress",
                        ) {
                            println!("Expected error occurred: {}", error);
                            return;
                        } else {
                            panic!("Unexpected error has occured {}.", error);
                        }
                    }
                },
            }

            // If this job exists, go again using new params, but also just wait a bit
            attempt += 1;
            thread::sleep(Duration::from_millis(2000));
        }

        let cancel_result = client.cancel_asvo_job(new_job_id.expect("No jobid was returned!"));

        assert!(cancel_result.is_ok());
        assert!(cancel_result.unwrap().unwrap() == new_job_id.unwrap());
    }

    #[test]
    fn test_submit_vis_as_tar() {
        let client = AsvoClient::new().unwrap();
        let obs_id = Obsid::validate(1343457784).unwrap();
        let delivery = Delivery::Scratch;
        let delivery_format: Option<DeliveryFormat> = Some(DeliveryFormat::Tar);
        let allow_resubmit: bool = false;

        let vis_job = client.submit_vis(obs_id, delivery, delivery_format, allow_resubmit);
        match vis_job {
            Ok(_) => (),
            Err(error) => match error {
                AsvoError::BadStatus { code, message } => {
                    println!("Got return code {} with message: {}", code, message)
                }
                _ => {
                    if error.to_string().contains(
                        "Your job cannot be submitted as there is a full outage in progress",
                    ) {
                        println!("Expected error occurred: {}", error);
                    } else {
                        panic!("Unexpected error has occured {}.", error);
                    }
                }
            },
        }
    }

    #[test]
    fn test_submit_conv_as_tar() {
        let client = AsvoClient::new().unwrap();
        let obs_id = Obsid::validate(1343457784).unwrap();
        let delivery = Delivery::Scratch;
        let delivery_format: Option<DeliveryFormat> = Some(DeliveryFormat::Tar);
        let job_params = BTreeMap::new();
        let allow_resubmit: bool = false;

        let conv_job = client.submit_conv(
            obs_id,
            delivery,
            delivery_format,
            &job_params,
            allow_resubmit,
        );
        match conv_job {
            Ok(_) => (),
            Err(error) => match error {
                AsvoError::BadStatus { code, message } => {
                    println!("Got return code {} with message: {}", code, message)
                }
                _ => {
                    if error.to_string().contains(
                        "Your job cannot be submitted as there is a full outage in progress",
                    ) {
                        println!("Expected error occurred: {}", error);
                    } else {
                        panic!("Unexpected error has occured {}.", error);
                    }
                }
            },
        }
    }

    #[test]
    fn test_submit_meta_as_tar() {
        let client = AsvoClient::new().unwrap();
        let obs_id = Obsid::validate(1343457784).unwrap();
        let delivery = Delivery::Scratch;
        let delivery_format: Option<DeliveryFormat> = Some(DeliveryFormat::Tar);
        let allow_resubmit: bool = false;

        let meta_job = client.submit_meta(obs_id, delivery, delivery_format, allow_resubmit);
        match meta_job {
            Ok(_) => (),
            Err(error) => match error {
                AsvoError::BadStatus { code, message } => {
                    println!("Got return code {} with message: {}", code, message)
                }
                _ => {
                    if error.to_string().contains(
                        "Your job cannot be submitted as there is a full outage in progress",
                    ) {
                        println!("Expected error occurred: {}", error);
                    } else {
                        panic!("Unexpected error has occured {}.", error);
                    }
                }
            },
        }
    }

    /* TODO: uncomment once MWA ASVO server supports delivery to DUG
    #[test]
    fn test_submit_meta_to_dug() {
        let client = AsvoClient::new().unwrap();
        let obs_id = Obsid::validate(1343457784).unwrap();
        let delivery = Delivery::Dug;
        let delivery_format: Option<DeliveryFormat> = None;
        let allow_resubmit: bool = false;

        let meta_job = client.submit_meta(obs_id, delivery, delivery_format, allow_resubmit);
        match meta_job {
            Ok(_) => (),
            Err(error) => match error {
                AsvoError::BadStatus {
                    code: _,
                    message: _,
                } => (),
                _ => panic!("Unexpected error has occured."),
            },
        }
    }*/

    #[test]
    fn test_submit_volt() {
        let client = AsvoClient::new().unwrap();
        // NOTE: this obs_id is a voltage observation, however for this test to pass,
        // You must have your pawsey_group set in your MWA ASVO profile to mwaops or mwavcs (contact an Admin to have this done).
        let obs_id = Obsid::validate(1290094336).unwrap();
        let offset: i32 = 0; // This will attempt to get data from GPS TIME: 1290094336
        let duration: i32 = 1; // This will attempt to get data up to GPS TIME: 1290094336
        let from_chan: Option<i32> = None;
        let to_chan: Option<i32> = None;
        let delivery = Delivery::Scratch;
        let allow_resubmit: bool = false;

        let volt_job = client.submit_volt(
            obs_id,
            delivery,
            offset,
            duration,
            from_chan,
            to_chan,
            allow_resubmit,
        );
        match volt_job {
            Ok(_) => (),
            Err(error) => match error {
                AsvoError::BadStatus { code, message } => {
                    println!("Got return code {} with message: {}", code, message)
                }
                _ => {
                    if error.to_string().contains(
                        "Your job cannot be submitted as there is a full outage in progress",
                    ) {
                        println!("Expected error occurred: {}", error);
                    } else {
                        panic!("Unexpected error has occured {}.", error);
                    }
                }
            },
        }
    }

    #[test]
    fn test_submit_volt_range() {
        let client = AsvoClient::new().unwrap();
        // NOTE: this obs_id is a voltage observation, however for this test to pass,
        // You must have your pawsey_group set in your MWA ASVO profile to mwaops or mwavcs (contact an Admin to have this done).
        let obs_id = Obsid::validate(1384018160).unwrap();
        let offset: i32 = 64; // This will attempt to get data from GPS TIME: 1384018224
        let duration: i32 = 8; // This will attempt to get data up to GPS TIME: 1384018232
        let from_chan: Option<i32> = Some(109);
        let to_chan: Option<i32> = Some(109);
        let delivery = Delivery::Scratch;
        let allow_resubmit: bool = false;

        let volt_job = client.submit_volt(
            obs_id,
            delivery,
            offset,
            duration,
            from_chan,
            to_chan,
            allow_resubmit,
        );
        match volt_job {
            Ok(_) => (),
            Err(error) => match error {
                AsvoError::BadStatus { code, message } => {
                    println!("Got return code {} with message: {}", code, message)
                }
                _ => {
                    if error.to_string().contains(
                        "Your job cannot be submitted as there is a full outage in progress",
                    ) {
                        println!("Expected error occurred: {}", error);
                    } else {
                        panic!("Unexpected error has occured {}.", error);
                    }
                }
            },
        }
    }
}
