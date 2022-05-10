use anyhow::Result;
use clap::Parser;
use clap_verbosity_flag;
use elementtree::Element;
use std::collections::{HashMap, HashSet};
use std::fs::{create_dir_all, read_to_string, write, File};
use std::hash::{Hash, Hasher};
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

/// Parse header and separator and return {field: (start, end)}
fn parse_header_and_separator(header: &str, separator: &str) -> HashMap<String, (usize, usize)> {
    let headers: Vec<&str> = header.split_whitespace().collect();
    let separators = separator.split_whitespace();
    let mut field_to_start_end: HashMap<String, (usize, usize)> = HashMap::new();
    let mut column = 0;
    for (ii, hyphens) in separators.enumerate() {
        let field = headers[ii];
        let start = column;
        let width = hyphens.len();
        let end = column + width;
        field_to_start_end.insert(field.to_string(), (start, end));
        column += width + 1
    }
    return field_to_start_end;
}

#[derive(Debug, Eq, PartialEq)]
struct World {
    sector: Sector,
    hex: String,
    name: String,
    uwp: String,
    trade_classifications: HashSet<String>,
    importance: u64,
    economic: String,
    cultural: String,
    nobles: String,
    bases: HashSet<String>,
    zone: String,
    pbg: String,
    worlds: u64,
    allegience: String,
    stars: Vec<String>,
    xboat_routes: HashSet<World>,
    major_routes: HashSet<World>,
    main_routes: HashSet<World>,
    intermediate_routes: HashSet<World>,
    feeder_routes: HashSet<World>,
    minor_routes: HashSet<World>,
    neighbors1: HashSet<World>,
    neighbors2: HashSet<World>,
    neighbors3: HashSet<World>,
    index: Option<u64>,
}

impl Hash for World {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.hex.hash(state);
        self.name.hash(state);
    }
}

#[derive(Debug, Eq, PartialEq)]
struct Sector {
    names: Vec<String>,
    abbreviation: String,
    location: (i64, i64),
    subsector_letter_to_name: HashMap<String, String>,
    allegience_code_to_name: HashMap<String, String>,
    hex_to_world: HashMap<String, World>,
}

impl Sector {
    fn new(data_dir: &PathBuf, sector_name: String) -> Sector {
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

    fn parse_column_data(&self, data_dir: &PathBuf, sector_name: &str) -> Result<()> {
        let mut data_path = data_dir.clone();
        data_path.push(sector_name.to_owned() + ".sec");
        let blob = read_to_string(data_path)?;
        let mut header = "";
        let mut separator = "";
        let mut fields: HashMap<String, (usize, usize)>;
        for line in blob.lines() {
            if line.len() == 0 || line.starts_with("#") {
                continue;
            }
            if line.starts_with("Hex") {
                header = line;
            } else if line.starts_with("----") {
                separator = line;
                fields = parse_header_and_separator(header, separator);
            } else {
                // TODO
                //let world = World::new(line, fields, self);
                //self.hex_to_world.insert(world.hex, world);
            }
        }

        Ok(())
    }

    /// Parse Xboat routes from xml
    /// Must be called after all Sectors and Worlds are built
    fn parse_xml_routes(
        &self,
        data_dir: &PathBuf,
        location_to_sector: &HashMap<(i64, i64), Sector>,
    ) -> Result<()> {
        let mut xml_path = data_dir.clone();
        xml_path.push(self.name().to_owned() + ".xml");
        let xml_file = File::open(xml_path)?;
        let root = Element::from_reader(xml_file)?;
        let routes_opt = root.find("Routes");
        if let Some(routes_element) = routes_opt {
            let route_elements = routes_element.find_all("Route");
            for route_element in route_elements {
                let start_hex_opt = route_element.get_attr("Start");
                if let Some(start_hex) = start_hex_opt {
                    let end_hex_opt = route_element.get_attr("End");
                    if let Some(end_hex) = end_hex_opt {
                        let start_offset_x_opt = route_element.get_attr("StartOffsetX");
                        let start_offset_x = 0;
                        if let Some(start_offset_x) = start_offset_x_opt {};
                        let start_offset_y_opt = route_element.get_attr("StartOffsetY");
                        let start_offset_y = 0;
                        if let Some(start_offset_y) = start_offset_y_opt {}
                        let end_offset_x_opt = route_element.get_attr("EndOffsetX");
                        let end_offset_x = 0;
                        if let Some(end_offset_x) = end_offset_x_opt {}
                        let end_offset_y_opt = route_element.get_attr("EndOffsetY");
                        let end_offset_y = 0;
                        if let Some(end_offset_y) = end_offset_y_opt {}
                        let start_sector_opt = location_to_sector.get(&(
                            self.location.0 + start_offset_x,
                            self.location.1 + start_offset_y,
                        ));
                        let end_sector_opt = location_to_sector.get(&(
                            self.location.0 + end_offset_x,
                            self.location.1 + end_offset_y,
                        ));
                        if let Some(start_sector) = start_sector_opt {
                            if let Some(end_sector) = end_sector_opt {
                                let start_world_opt = start_sector.hex_to_world.get(start_hex);
                                let end_world_opt = end_sector.hex_to_world.get(end_hex);
                                if let Some(start_world) = start_world_opt {
                                    if let Some(end_world) = end_world_opt {
                                        // TODO sort these out
                                        //start_world.xboat_routes.insert(end_world);
                                        //end_world.xboat_routes.insert(start_world);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(())
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
    for sector_name in sector_names {
        let sector = Sector::new(&data_dir, sector_name);
        location_to_sector.insert(sector.location, sector);
    }
    for sector in location_to_sector.values() {
        sector.parse_xml_routes(&data_dir, &location_to_sector);
    }

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

    //#[test]
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

    #[test]
    fn test_parse_header_and_separator() -> Result<()> {
        let header = concat!(
            r"Hex  Name                 UWP       ",
            r"Remarks                                  {Ix}   (Ex)   ",
            r"[Cx]   N     B  Z PBG W  A    Stellar       "
        )
        .to_owned();

        let separator = concat!(
            r"---- -------------------- --------- ",
            r"---------------------------------------- ------ ------- ",
            r"------ ----- -- - --- -- ---- --------------"
        )
        .to_owned();

        let fields = parse_header_and_separator(&header, &separator);
        assert_eq!(fields.len(), 14);
        assert_eq!(fields.get("Hex"), Some(&(0, 4)));
        assert_eq!(fields.get("Name"), Some(&(5, 25)));
        assert_eq!(fields.get("UWP"), Some(&(26, 35)));
        assert_eq!(fields.get("Remarks"), Some(&(36, 76)));
        assert_eq!(fields.get("{Ix}"), Some(&(77, 83)));
        assert_eq!(fields.get("(Ex)"), Some(&(84, 91)));
        assert_eq!(fields.get("[Cx]"), Some(&(92, 98)));
        assert_eq!(fields.get("N"), Some(&(99, 104)));
        assert_eq!(fields.get("B"), Some(&(105, 107)));
        assert_eq!(fields.get("Z"), Some(&(108, 109)));
        assert_eq!(fields.get("PBG"), Some(&(110, 113)));
        assert_eq!(fields.get("W"), Some(&(114, 116)));
        assert_eq!(fields.get("A"), Some(&(117, 121)));
        assert_eq!(fields.get("Stellar"), Some(&(122, 136)));

        Ok(())
    }
}
