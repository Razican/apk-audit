use std::fs;
use std::collections::BTreeSet;
use std::path::Path;
use std::result::Result as StdResult;
use std::error::Error as StdError;

use serde::ser::{Serialize, Serializer};
use chrono::Local;

mod utils;
mod handlebars_helpers;
mod report;

pub use self::utils::{Vulnerability, split_indent, html_escape};
use self::utils::FingerPrint;

use {Config, Result, Criticality, print_error, print_warning, get_package_name};

use results::report::{Json, HandlebarsReport};
use results::report::Report;

pub struct Results {
    app_package: String,
    app_label: String,
    app_description: String,
    app_version: String,
    app_version_num: i32,
    app_min_sdk: i32,
    app_target_sdk: Option<i32>,
    app_fingerprint: FingerPrint,
    #[allow(unused)]
    certificate: String,
    warnings: BTreeSet<Vulnerability>,
    low: BTreeSet<Vulnerability>,
    medium: BTreeSet<Vulnerability>,
    high: BTreeSet<Vulnerability>,
    critical: BTreeSet<Vulnerability>,
}

impl Results {
    pub fn init<P: AsRef<Path>>(config: &Config, package: P) -> Option<Results> {
        let path = config.get_results_folder().join(get_package_name(package.as_ref()));
        if !path.exists() || config.is_force() {
            if path.exists() {
                if let Err(e) = fs::remove_dir_all(&path) {
                    print_error(format!("An unknown error occurred when trying to delete the \
                                         results folder: {}",
                                        e),
                                config.is_verbose());
                    return None;
                }
            }

            let fingerprint = match FingerPrint::new(package) {
                Ok(f) => f,
                Err(e) => {
                    print_error(format!("An error occurred when trying to fingerprint the \
                                         application: {}",
                                        e),
                                config.is_verbose());
                    return None;
                }
            };
            if config.is_verbose() {
                println!("The results struct has been created. All the vulnerabilitis will now \
                          be recorded and when the analysis ends, they will be written to result \
                          files.");
            } else if !config.is_quiet() {
                println!("Results struct created.");
            }
            Some(Results {
                app_package: String::new(),
                app_label: String::new(),
                app_description: String::new(),
                app_version: String::new(),
                app_version_num: 0,
                app_min_sdk: 0,
                app_target_sdk: None,
                app_fingerprint: fingerprint,
                certificate: String::new(),
                warnings: BTreeSet::new(),
                low: BTreeSet::new(),
                medium: BTreeSet::new(),
                high: BTreeSet::new(),
                critical: BTreeSet::new(),
            })
        } else {
            if config.is_verbose() {
                println!("The results for this application have already been generated. No need \
                          to generate them again.");
            } else {
                println!("Skipping result generation.");
            }
            None
        }
    }

    pub fn set_app_package<S: Into<String>>(&mut self, package: S) {
        self.app_package = package.into();
    }

    pub fn get_app_package(&self) -> &str {
        &self.app_package
    }

    #[cfg(feature = "certificate")]
    pub fn set_certificate<S: Into<String>>(&mut self, certificate: S) {
        self.certificate = certificate.into();
    }

    pub fn set_app_label<S: Into<String>>(&mut self, label: S) {
        self.app_label = label.into();
    }

    pub fn set_app_description<S: Into<String>>(&mut self, description: S) {
        self.app_description = description.into();
    }

    pub fn set_app_version<S: Into<String>>(&mut self, version: S) {
        self.app_version = version.into();
    }

    pub fn set_app_version_num(&mut self, version: i32) {
        self.app_version_num = version;
    }

    pub fn set_app_min_sdk(&mut self, sdk: i32) {
        self.app_min_sdk = sdk;
    }

    pub fn set_app_target_sdk(&mut self, sdk: i32) {
        self.app_target_sdk = Some(sdk);
    }

    pub fn add_vulnerability(&mut self, vuln: Vulnerability) {
        match vuln.get_criticality() {
            Criticality::Warning => {
                self.warnings.insert(vuln);
            }
            Criticality::Low => {
                self.low.insert(vuln);
            }
            Criticality::Medium => {
                self.medium.insert(vuln);
            }
            Criticality::High => {
                self.high.insert(vuln);
            }
            Criticality::Critical => {
                self.critical.insert(vuln);
            }
        }
    }

    pub fn generate_report<S: AsRef<str>>(&self, config: &Config, package: S) -> Result<bool> {
        let path = config.get_results_folder().join(&self.app_package);
        if config.is_force() || !path.exists() {
            if path.exists() {
                if config.is_verbose() {
                    println!("The application results folder exists. But no more…");
                }

                if let Err(e) = fs::remove_dir_all(&path) {
                    print_warning(format!("There was an error when removing the results \
                                           folder: {}",
                                          e.description()),
                                  config.is_verbose());
                }
            }
            if config.is_verbose() {
                println!("Starting report generation. First we'll create the results folder.");
            }
            fs::create_dir_all(&path)?;
            if config.is_verbose() {
                println!("Results folder created. Time to create the reports.");
            }

            if config.has_to_generate_json() {
                let mut json_reporter = Json::new();

                if let Err(e) = json_reporter.generate(config, self) {
                    print_warning(format!("There was en error generating JSON report: {}", e),
                                  config.is_verbose());
                }

                if config.is_verbose() {
                    println!("JSON report generated.");
                    println!("");
                }
            }

            if config.has_to_generate_html() {
                let handelbars_report_result = HandlebarsReport::new(config.get_template_path(),
                                                                     package.as_ref().to_owned());

                if let Ok(mut handlebars_reporter) = handelbars_report_result {
                    if let Err(e) = handlebars_reporter.generate(config, self) {
                        print_warning(format!("There was en error generating HTML report: {}", e),
                                      config.is_verbose());
                    }

                    if config.is_verbose() {
                        println!("HTML report generated.");
                    }
                }
            }

            Ok(true)
        } else {
            if config.is_verbose() {
                println!("Seems that the report has already been generated. There is no need to \
                          o it again.");
            } else {
                println!("Skipping report generation.");
            }
            Ok(false)
        }
    }
}

impl Serialize for Results {
    fn serialize<S>(&self, serializer: &mut S) -> StdResult<(), S::Error>
        where S: Serializer
    {
        let now = Local::now();
        let mut state = serializer.serialize_struct("Results", 22)?;

        serializer.serialize_struct_elt(&mut state, "super_version", crate_version!())?;
        serializer.serialize_struct_elt(&mut state, "now", &now)?;
        serializer.serialize_struct_elt(&mut state, "now_rfc2822", now.to_rfc2822())?;
        serializer.serialize_struct_elt(&mut state, "now_rfc3339", now.to_rfc3339())?;

        serializer.serialize_struct_elt(&mut state, "app_package", &self.app_package)?;
        serializer.serialize_struct_elt(&mut state, "app_version", &self.app_version)?;
        serializer.serialize_struct_elt(&mut state, "app_version_number", &self.app_version_num)?;
        serializer.serialize_struct_elt(&mut state, "app_fingerprint", &self.app_fingerprint)?;
        serializer.serialize_struct_elt(&mut state, "certificate", &self.certificate)?;

        serializer.serialize_struct_elt(&mut state, "app_min_sdk", &self.app_min_sdk)?;
        serializer.serialize_struct_elt(&mut state, "app_target_sdk", &self.app_target_sdk)?;

        serializer.serialize_struct_elt(&mut state,
                                  "total_vulnerabilities",
                                  self.low.len() + self.medium.len() + self.high.len() +
                                  self.critical.len())?;
        serializer.serialize_struct_elt(&mut state, "criticals", &self.critical)?;
        serializer.serialize_struct_elt(&mut state, "criticals_len", self.critical.len())?;
        serializer.serialize_struct_elt(&mut state, "highs", &self.high)?;
        serializer.serialize_struct_elt(&mut state, "highs_len", self.high.len())?;
        serializer.serialize_struct_elt(&mut state, "mediums", &self.medium)?;
        serializer.serialize_struct_elt(&mut state, "mediums_len", self.medium.len())?;
        serializer.serialize_struct_elt(&mut state, "lows", &self.low)?;
        serializer.serialize_struct_elt(&mut state, "lows_len", self.low.len())?;
        serializer.serialize_struct_elt(&mut state, "warnings", &self.warnings)?;
        serializer.serialize_struct_elt(&mut state, "warnings_len", self.warnings.len())?;

        serializer.serialize_struct_end(state)?;
        Ok(())
    }
}
