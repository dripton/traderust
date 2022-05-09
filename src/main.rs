use anyhow::{Error, Result};
use clap::Parser;
use clap_verbosity_flag;
use elementtree::Element;
use std::collections::HashMap;
use std::fs::{create_dir_all, write, File};
use std::path::PathBuf;
extern crate reqwest;
use url::Url;

#[derive(Debug, Parser)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(short, long, multiple_occurrences = true, required = true)]
    sector: Vec<String>,

    /// Path to input matrix in numpy ndy format
    // TODO Make this optional and use a tempdir if not provided
    #[clap(short, long, required = true)]
    data_directory: PathBuf,

    /// Path to output distance and predecessor matrixes in numpy ndz format
    #[clap(short, long, default_value = "/var/tmp")]
    output_directory: PathBuf,

    #[clap(flatten)]
    verbose: clap_verbosity_flag::Verbosity,
}

fn download_sector_data(data_dir: &PathBuf, sector_names: &Vec<String>) -> Result<()> {
    for sector_name in sector_names {
        let sector_data_filename = sector_name.to_owned() + ".sec";
        let mut data_path = data_dir.clone();
        data_path.push(sector_data_filename);
        let sector_xml_filename = sector_name.to_owned() + ".xml";
        let mut metadata_path = data_dir.clone();
        metadata_path.push(sector_xml_filename);
        let base_url = Url::parse("https://travellermap.com/data/")?;
        if !data_path.exists() {
            let data_url = base_url.join(sector_name)?;
            let body = reqwest::blocking::get(data_url)?.text()?;
            write(data_path, body)?;
        }
        if !metadata_path.exists() {
            let metadata_url = base_url.join(&(sector_name.to_owned() + "/metadata"))?;
            let body = reqwest::blocking::get(metadata_url)?.text()?;
            write(metadata_path, body)?;
        }
    }
    Ok(())
}

#[derive(Debug)]
struct World {
    // TODO
}

#[derive(Debug)]
struct Sector {
    names: Vec<String>,
    abbreviation: String,
    location: (i64, i64),
    subsector_letter_to_name: HashMap<String, String>,
    allegience_code_to_name: HashMap<String, String>,
    hex_to_world: HashMap<String, World>,
}

impl Sector {
    fn new(
        data_dir: &PathBuf,
        sector_name: String,
        location_to_sector: &mut HashMap<(i64, i64), Sector>,
    ) -> Sector {
        let names = Vec::new();
        let abbreviation = "".to_string();
        let location = (-1, -1);
        let subsector_letter_to_name = HashMap::new();
        let allegience_code_to_name = HashMap::new();
        let hex_to_world = HashMap::new();
        let mut sector = Sector {
            names,
            abbreviation,
            location,
            subsector_letter_to_name,
            allegience_code_to_name,
            hex_to_world,
        };

        sector.parse_xml_metadata(&data_dir, &sector_name);
        sector.parse_column_data(&data_dir, &sector_name);
        sector
    }

    fn parse_xml_metadata(&mut self, data_dir: &PathBuf, sector_name: &str) -> Result<()> {
        let mut xml_path = data_dir.clone();
        xml_path.push(sector_name.to_owned() + ".xml");
        let xml_file = File::open(xml_path)?;
        let root = Element::from_reader(xml_file)?;

        let abbreviation_opt = root.get_attr("Abbreviation");
        if let Some(abbreviation) = abbreviation_opt {
            self.abbreviation = abbreviation.to_string();
        }

        let mut x = i64::MAX;
        let x_opt = root.find("X");
        if let Some(x_element) = x_opt {
            let x_text = x_element.text();
            x = x_text.parse()?;
        }
        let mut y = i64::MAX;
        let y_opt = root.find("y");
        if let Some(y_element) = y_opt {
            let y_text = y_element.text();
            y = y_text.parse()?;
        }
        self.location = (x, y);

        let name_elements = root.find_all("Name");
        for name_element in name_elements {
            if name_element.text().len() > 0 {
                self.names.push(name_element.text().to_string());
            }
        }

        let subsectors_opt = root.find("Subsectors");
        if let Some(subsectors_element) = subsectors_opt {
            let subsector_elements = subsectors_element.find_all("Subsector");
            for subsector_element in subsector_elements {
                let letter_opt = subsector_element.get_attr("Index");
                if let Some(letter) = letter_opt {
                    let subsector_name = subsector_element.text().to_string();
                    if subsector_name.len() > 0 {
                        self.subsector_letter_to_name
                            .insert(letter.to_string(), subsector_name);
                    }
                }
            }
        }

        let allegiances_opt = root.find("Allegiances");
        if let Some(allegiances_element) = allegiances_opt {
            let allegience_elements = allegiances_element.find_all("Allegience");
            for allegience_element in allegience_elements {
                let code_opt = allegience_element.get_attr("Code");
                if let Some(code) = code_opt {
                    let allegience_name = allegience_element.text().to_string();
                    if allegience_name.len() > 0 {
                        self.allegience_code_to_name
                            .insert(code.to_string(), allegience_name);
                    }
                }
            }
        }

        Ok(())
    }

    fn parse_column_data(&self, data_dir: &PathBuf, sector_name: &str) {
        // TODO
    }

    fn populate_neighbors(&self) {
        // TODO
    }

    fn name(&self) -> &str {
        &self.names[0]
    }
}

fn main() -> Result<()> {
    let args = Args::parse();
    let output_dir = args.output_directory;
    let data_dir = args.data_directory;
    let sector_names = args.sector;

    create_dir_all(&output_dir)?;
    create_dir_all(&data_dir)?;

    download_sector_data(&data_dir, &sector_names)?;
    let mut location_to_sector: HashMap<(i64, i64), Sector> = HashMap::new();

    // TODO
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::fs::read_dir;
    use std::io;
    use tempfile::tempdir;

    #[test]
    fn test_download_sector_data() -> Result<()> {
        let mut expected_filenames = Vec::new();
        let sector_names = vec![
            "Deneb".to_string(),
            "Gvurrdon".to_string(),
            "Spinward Marches".to_string(),
        ];
        for sector_name in &sector_names {
            expected_filenames.push(sector_name.to_owned() + ".sec");
            expected_filenames.push(sector_name.to_owned() + ".xml");
        }
        expected_filenames.sort();

        let temp_dir = tempdir()?;
        download_sector_data(&(temp_dir.path().to_path_buf()), &sector_names)?;
        let found_filename_results: Vec<Result<OsString, io::Error>> = read_dir(&temp_dir)?
            .map(|res| res.map(|e| e.file_name()))
            .collect();
        let mut found_os_filenames: Vec<OsString> = Vec::new();
        for res in found_filename_results {
            if let Ok(filename) = res {
                found_os_filenames.push(filename);
            }
        }

        let mut found_filenames: Vec<String> = Vec::new();
        for osstr in found_os_filenames {
            let opt = osstr.to_str();
            if let Some(st) = opt {
                found_filenames.push(st.to_string());
            }
        }
        found_filenames.sort();

        assert_eq!(expected_filenames, found_filenames);

        temp_dir.close()?;

        Ok(())
    }
}
