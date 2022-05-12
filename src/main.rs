use anyhow::Result;
use clap::Parser;
use clap_verbosity_flag;
use elementtree::Element;
use std::collections::{HashMap, HashSet};
use std::fs::{create_dir_all, read_to_string, write, File};
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
#[macro_use]
extern crate lazy_static;
extern crate reqwest;
use substring::Substring;
use tempfile::tempdir;
use url::Url;

#[derive(Debug, Parser)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(short, long, multiple_occurrences = true, required = true)]
    sector: Vec<String>,

    /// Path to input matrix in numpy ndy format
    #[clap(short, long)]
    data_directory: Option<PathBuf>,

    /// Path to output distance and predecessor matrixes in numpy ndz format
    #[clap(short, long, default_value = "/var/tmp")]
    output_directory: PathBuf,

    #[clap(flatten)]
    verbose: clap_verbosity_flag::Verbosity,
}

lazy_static! {

    static ref SQRT3: f64 = f64::powf(3.0, 0.5);

    static ref STARPORT_TRAVELLER_TO_GURPS: HashMap<String, String> = {
        let mut sttg: HashMap<String, String> = HashMap::new();
        sttg.insert("A".to_string(), "V".to_string());

        sttg.insert("C".to_string(), "III".to_string());
        sttg.insert("D".to_string(), "II".to_string());
        sttg.insert("E".to_string(), "I".to_string());
        sttg.insert("X".to_string(), "0".to_string());
        sttg
    };


    static ref TECH_LEVEL_TRAVELLER_TO_GURPS: HashMap<u64, u64> = {
        let mut tttg: HashMap<u64, u64> = HashMap::new();
        tttg.insert(0, 2); // actually 1-3
        tttg.insert(1, 4);
        tttg.insert(2, 5);
        tttg.insert(3, 5);
        tttg.insert(4, 5);
        tttg.insert(5, 6);
        tttg.insert(6, 6);
        tttg.insert(7, 7);
        tttg.insert(8, 8);
        tttg.insert(9, 9);
        tttg.insert(10, 9);
        tttg.insert(11, 9);
        tttg.insert(12, 10);
        tttg.insert(13, 10);
        tttg.insert(14, 11);
        tttg.insert(15, 12);
        tttg.insert(16, 13);
        tttg.insert(17, 13);
        tttg
    };
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

/// Absolute coordinates
/// x is an integer
/// y is an integer
/// half_y shows if y should actually be pushed down half a hex
/// This is needed because floats can't be hash keys
#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq)]
struct Coords {
    x: i64,
    y: i64,
    half_y: bool,
}

impl Coords {
    fn new(xf: f64, yf: f64) -> Coords {
        let x = xf as i64;
        let y = yf as i64;
        let half_y = yf - y as f64 != 0.0;
        Coords { x, y, half_y }
    }
}

impl From<Coords> for (f64, f64) {
    fn from(coords: Coords) -> (f64, f64) {
        let fx = coords.x as f64;
        let mut fy = coords.y as f64;
        if coords.half_y {
            fy += 0.5;
        }
        (fx, fy)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct World {
    sector_location: (i64, i64),
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
    xboat_routes: HashSet<Coords>,
    major_routes: HashSet<Coords>,
    main_routes: HashSet<Coords>,
    intermediate_routes: HashSet<Coords>,
    feeder_routes: HashSet<Coords>,
    minor_routes: HashSet<Coords>,
    neighbors1: HashSet<Coords>,
    neighbors2: HashSet<Coords>,
    neighbors3: HashSet<Coords>,
    index: Option<u64>,
}

impl World {
    fn new(
        line: String,
        fields: &HashMap<String, (usize, usize)>,
        sector_location: (i64, i64),
    ) -> World {
        let mut hex = "".to_string();
        let mut name = "".to_string();
        let mut uwp = "".to_string();
        let mut trade_classifications = HashSet::new();
        let mut importance = 0;
        let mut economic = "".to_string();
        let mut cultural = "".to_string();
        let mut nobles = "".to_string();
        let mut bases = HashSet::new();
        let mut zone = "G".to_string();
        let mut pbg = "".to_string();
        let mut worlds = 0;
        let mut allegience = "".to_string();
        let mut stars = Vec::new();
        let xboat_routes = HashSet::new();
        let major_routes = HashSet::new();
        let main_routes = HashSet::new();
        let intermediate_routes = HashSet::new();
        let feeder_routes = HashSet::new();
        let minor_routes = HashSet::new();
        let neighbors1 = HashSet::new();
        let neighbors2 = HashSet::new();
        let neighbors3 = HashSet::new();
        let index = None;

        for (field, (start, end)) in fields.iter() {
            let value_opt = line.get(*start..*end);
            if let Some(value) = value_opt {
                match field.as_str() {
                    "Hex" => hex = value.to_string(),
                    "Name" => name = value.trim().to_string(),
                    "UWP" => uwp = value.to_string(),
                    "Remarks" => {
                        for tc in value.trim().split_whitespace() {
                            trade_classifications.insert(tc.to_string());
                        }
                    }
                    "{Ix}" => {
                        let trimmed = value
                            .trim_matches(|c| c == '{' || c == '}' || c == ' ')
                            .to_string();
                        if trimmed.len() > 0 {
                            if let Ok(val) = trimmed.parse() {
                                importance = val;
                            }
                        }
                    }
                    "(Ex)" => economic = value.trim_matches(|c| c == '(' || c == ')').to_string(),
                    "[Cx]" => cultural = value.trim_matches(|c| c == '[' || c == ']').to_string(),
                    "N" => nobles = value.trim_matches(|c| c == ' ' || c == '-').to_string(),
                    "B" => {
                        let trimmed = value.trim_matches(|c| c == ' ' || c == '-').to_string();
                        if trimmed.len() > 0 {
                            for ch in trimmed.chars() {
                                bases.insert(ch.to_string());
                            }
                        }
                    }
                    "Z" => {
                        let trimmed = value.trim_matches(|c| c == ' ' || c == '-').to_string();
                        if trimmed.len() > 0 {
                            zone = trimmed;
                        }
                    }
                    "PBG" => pbg = value.trim().to_string(),
                    "W" => {
                        let trimmed = value
                            .trim_matches(|c| c == '{' || c == '}' || c == ' ')
                            .to_string();
                        if trimmed.len() > 0 {
                            if let Ok(val) = trimmed.parse() {
                                worlds = val;
                            }
                        }
                    }
                    "A" => allegience = value.to_string(),
                    "Stellar" => {
                        let parts: Vec<&str> = value.trim().split_whitespace().collect();
                        let mut ii = 0;
                        while ii < parts.len() {
                            let star = parts[ii];
                            if star == "BD" || star == "D" {
                                stars.push(star.to_owned());
                                ii += 1;
                            } else {
                                stars.push(star.to_owned() + " " + &parts[ii + 1]);
                                ii += 2;
                            }
                        }
                    }
                    &_ => (),
                }
            }
        }

        let world = World {
            sector_location,
            hex,
            name,
            uwp,
            trade_classifications,
            importance,
            economic,
            cultural,
            nobles,
            bases,
            zone,
            pbg,
            worlds,
            allegience,
            stars,
            xboat_routes,
            major_routes,
            main_routes,
            intermediate_routes,
            feeder_routes,
            minor_routes,
            neighbors1,
            neighbors2,
            neighbors3,
            index,
        };
        world
    }

    /// Find and cache all neighbors within 3 hexes.
    ///
    /// This must be run after all Sectors and Worlds are mostly initialized.
    fn populate_neighbors(&mut self, coords_to_world: &HashMap<Coords, World>) {
        if !self.can_refuel() {
            return;
        }
        let (x, y) = <(f64, f64)>::from(self.get_coords());
        let mut xx = x - 3.0;
        while xx <= x + 3.0 {
            let mut yy = y - 3.0;
            while yy <= y + 3.0 {
                let world_opt = coords_to_world.get(&Coords::new(xx, yy));
                if let Some(world) = world_opt {
                    if world != self && world.can_refuel() {
                        let distance = self.straight_line_distance(world);
                        match distance {
                            1 => self.neighbors1.insert(world.get_coords()),
                            2 => self.neighbors2.insert(world.get_coords()),
                            3 => self.neighbors3.insert(world.get_coords()),
                            _ => false,
                        };
                    }
                }
                yy += 0.5;
            }
            xx += 1.0;
        }
    }

    fn starport(&self) -> String {
        return self.uwp.substring(0, 1).to_string();
    }

    fn g_starport(&self) -> String {
        let starport = self.starport();
        let opt = STARPORT_TRAVELLER_TO_GURPS.get(&starport);
        return opt.unwrap().to_string();
    }

    fn hydrosphere(&self) -> String {
        return self.uwp.substring(3, 4).to_string();
    }

    fn gas_giants(&self) -> String {
        return self.pbg.substring(2, 3).to_string();
    }

    fn can_refuel(&self) -> bool {
        return self.gas_giants() != "0"
            || (self.zone != "R" && (self.starport() != "E" && self.starport() != "X")
                || self.hydrosphere() != "0");
    }

    /// Return double the actual coordinates, as we have half-hexes vertically.
    fn get_coords(&self) -> Coords {
        let hex = &self.hex;
        let location = self.sector_location;
        let x: i64 = hex.substring(0, 2).parse::<i64>().unwrap() + 32 * location.0;
        let y: i64 = hex.substring(2, 4).parse::<i64>().unwrap() + 40 * location.1;
        let half_y = x & 1 == 0;
        return Coords { x, y, half_y };
    }

    fn straight_line_distance(&self, other: &World) -> u64 {
        let (x1, y1) = <(f64, f64)>::from(self.get_coords());
        let (x2, y2) = <(f64, f64)>::from(other.get_coords());
        let xdelta = f64::abs(x2 - x1);
        let mut ydelta = f64::abs(y2 - y1) - xdelta / 2.0;
        if ydelta < 0.0 {
            ydelta = 0.0;
        }
        return (f64::floor(xdelta + ydelta)) as u64;
    }
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
    hex_to_coords: HashMap<String, Coords>,
}

impl Sector {
    fn new(
        data_dir: &PathBuf,
        sector_name: String,
        coords_to_world: &mut HashMap<Coords, World>,
    ) -> Sector {
        let names = Vec::new();
        let abbreviation = "".to_string();
        let location = (-1, -1);
        let subsector_letter_to_name = HashMap::new();
        let allegience_code_to_name = HashMap::new();
        let hex_to_coords = HashMap::new();
        let mut sector = Sector {
            names,
            abbreviation,
            location,
            subsector_letter_to_name,
            allegience_code_to_name,
            hex_to_coords,
        };

        sector.parse_xml_metadata(&data_dir, &sector_name);
        sector.parse_column_data(&data_dir, &sector_name, coords_to_world);
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

    fn parse_column_data(
        &mut self,
        data_dir: &PathBuf,
        sector_name: &str,
        coords_to_world: &mut HashMap<Coords, World>,
    ) -> Result<()> {
        let mut data_path = data_dir.clone();
        data_path.push(sector_name.to_owned() + ".sec");
        let blob = read_to_string(data_path)?;
        let mut header = "";
        let mut separator = "";
        // We initialize fields here to make rustc happy, then overwrite it.
        let mut fields: HashMap<String, (usize, usize)> = HashMap::new();
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
                let world = World::new(line.to_string(), &fields, self.location);
                self.hex_to_coords
                    .insert(world.hex.clone(), world.get_coords());
                coords_to_world.insert(world.get_coords(), world);
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
        coords_to_world: &mut HashMap<Coords, World>,
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
                                if let Some(start_coords) =
                                    start_sector.hex_to_coords.get(start_hex)
                                {
                                    if let Some(end_coords) = end_sector.hex_to_coords.get(end_hex)
                                    {
                                        // Need to do these one at a time to avoid holding two
                                        // mutable references at once.
                                        if let Some(start_world) = coords_to_world.get(start_coords)
                                        {
                                            if let Some(end_world) =
                                                coords_to_world.get_mut(end_coords)
                                            {
                                                end_world.xboat_routes.insert(*start_coords);
                                            }
                                        }
                                        if let Some(end_world) = coords_to_world.get(end_coords) {
                                            if let Some(start_world) =
                                                coords_to_world.get_mut(start_coords)
                                            {
                                                start_world.xboat_routes.insert(*end_coords);
                                            }
                                        }
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

    fn name(&self) -> &str {
        &self.names[0]
    }
}

fn main() -> Result<()> {
    let args = Args::parse();
    let output_dir = args.output_directory;
    let temp_dir = tempdir()?;
    let mut data_dir: PathBuf = temp_dir.path().to_path_buf();
    if let Some(data_dir) = args.data_directory {};
    let sector_names = args.sector;

    create_dir_all(&output_dir)?;
    create_dir_all(&data_dir)?;

    download_sector_data(&data_dir, &sector_names)?;

    let mut location_to_sector: HashMap<(i64, i64), Sector> = HashMap::new();
    let mut coords_to_world: HashMap<Coords, World> = HashMap::new();
    for sector_name in sector_names {
        let sector = Sector::new(&data_dir, sector_name, &mut coords_to_world);
        location_to_sector.insert(sector.location, sector);
    }
    for sector in location_to_sector.values() {
        sector.parse_xml_routes(&data_dir, &location_to_sector, &mut coords_to_world);
    }
    {
        // Make a temporary clone to avoid having mutable and immutable refs.
        let coords_to_world2 = coords_to_world.clone();
        for world in coords_to_world.values_mut() {
            world.populate_neighbors(&coords_to_world2);
        }
    }

    temp_dir.close()?;

    // TODO
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::fs::read_dir;
    use std::io;

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
