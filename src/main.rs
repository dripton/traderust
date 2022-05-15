use anyhow::Result;
use clap::Parser;
use clap_verbosity_flag;
use elementtree::Element;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::fs::{create_dir_all, read_to_string, write, File};
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
#[macro_use]
extern crate lazy_static;
extern crate ndarray;
use ndarray::prelude::*;
extern crate reqwest;
use substring::Substring;
use tempfile::tempdir;
use url::Url;

mod apsp;
use apsp::{dijkstra, INFINITY};

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

const MAX_TECH_LEVEL: u32 = 17;
const MAX_POPULATION: u32 = 15;

lazy_static! {
    static ref SQRT3: f64 = f64::powf(3.0, 0.5);

    static ref STARPORT_TRAVELLER_TO_GURPS: HashMap<String, String> = {
        let mut sttg: HashMap<String, String> = HashMap::new();
        sttg.insert("A".to_string(), "V".to_string());
        sttg.insert("B".to_string(), "IV".to_string());
        sttg.insert("C".to_string(), "III".to_string());
        sttg.insert("D".to_string(), "II".to_string());
        sttg.insert("E".to_string(), "I".to_string());
        sttg.insert("X".to_string(), "0".to_string());
        sttg
    };

    static ref TECH_LEVEL_TRAVELLER_TO_GURPS: HashMap<u32, u64> = {
        let mut tttg: HashMap<u32, u64> = HashMap::new();
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

    static ref WTN_PORT_MODIFIER_TABLE: HashMap<(u64, String), f64> = {
        let mut wpmt: HashMap<(u64, String), f64> = HashMap::new();
        wpmt.insert((7, "V".to_string()), 0.0);
        wpmt.insert((7, "IV".to_string()), -1.0);
        wpmt.insert((7, "III".to_string()), -1.5);
        wpmt.insert((7, "II".to_string()), -2.0);
        wpmt.insert((7, "I".to_string()), -2.5);
        wpmt.insert((7, "0".to_string()), -5.0);
        wpmt.insert((6, "V".to_string()), 0.0);
        wpmt.insert((6, "IV".to_string()), -0.5);
        wpmt.insert((6, "III".to_string()), -1.0);
        wpmt.insert((6, "II".to_string()), -1.5);
        wpmt.insert((6, "I".to_string()), -2.0);
        wpmt.insert((6, "0".to_string()), -4.5);
        wpmt.insert((5, "V".to_string()), 0.0);
        wpmt.insert((5, "IV".to_string()), 0.0);
        wpmt.insert((5, "III".to_string()), -0.5);
        wpmt.insert((5, "II".to_string()), -1.0);
        wpmt.insert((5, "I".to_string()), -1.5);
        wpmt.insert((5, "0".to_string()), -4.0);
        wpmt.insert((4, "V".to_string()), 0.5);
        wpmt.insert((4, "IV".to_string()), 0.0);
        wpmt.insert((4, "III".to_string()), 0.0);
        wpmt.insert((4, "II".to_string()), -0.5);
        wpmt.insert((4, "I".to_string()), -1.0);
        wpmt.insert((4, "0".to_string()), -3.5);
        wpmt.insert((3, "V".to_string()), 0.5);
        wpmt.insert((3, "IV".to_string()), 0.5);
        wpmt.insert((3, "III".to_string()), 0.0);
        wpmt.insert((3, "II".to_string()), 0.0);
        wpmt.insert((3, "I".to_string()), -0.5);
        wpmt.insert((3, "0".to_string()), -3.0);
        wpmt.insert((2, "V".to_string()), 1.0);
        wpmt.insert((2, "IV".to_string()), 0.5);
        wpmt.insert((2, "III".to_string()), 0.5);
        wpmt.insert((2, "II".to_string()), 0.0);
        wpmt.insert((2, "I".to_string()), 0.0);
        wpmt.insert((2, "0".to_string()), -2.5);
        wpmt.insert((1, "V".to_string()), 1.0);
        wpmt.insert((1, "IV".to_string()), 1.0);
        wpmt.insert((1, "III".to_string()), 0.5);
        wpmt.insert((1, "II".to_string()), 0.0);
        wpmt.insert((1, "I".to_string()), 0.0);
        wpmt.insert((1, "0".to_string()), 0.0);
        wpmt.insert((0, "V".to_string()), 1.5);
        wpmt.insert((0, "IV".to_string()), 1.0);
        wpmt.insert((0, "III".to_string()), 1.0);
        wpmt.insert((0, "II".to_string()), 0.5);
        wpmt.insert((0, "I".to_string()), 0.5);
        wpmt.insert((0, "0".to_string()), 0.0);
        wpmt
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

/// Parse header and separator and return [(start, end, field)]
fn parse_header_and_separator(header: &str, separator: &str) -> Vec<(usize, usize, String)> {
    let headers: Vec<&str> = header.split_whitespace().collect();
    let separators = separator.split_whitespace();
    let mut fields: Vec<(usize, usize, String)> = Vec::new();
    let mut column = 0;
    for (ii, hyphens) in separators.enumerate() {
        let field = headers[ii];
        let start = column;
        let width = hyphens.len();
        let end = column + width;
        fields.push((start, end, field.to_string()));
        column += width + 1;
    }
    return fields;
}

/// Find minimum distances between all worlds, and predecessor paths.
/// Only use jumps of up to max_jump hexes, except along xboat routes.
/// Must be run after all neighbors are built.
fn populate_navigable_distances(
    sorted_coords: &Vec<Coords>,
    coords_to_world: &HashMap<Coords, World>,
    max_jump: u64,
) -> (Array2<i64>, Array2<i64>) {
    let num_worlds = sorted_coords.len();
    let mut np = Array2::<i64>::zeros((num_worlds, num_worlds));
    for (ii, coords) in sorted_coords.iter().enumerate() {
        let world_opt = coords_to_world.get(coords);
        if let Some(world) = world_opt {
            if max_jump >= 3 {
                for coords in &world.neighbors3 {
                    if let Some(neighbor) = coords_to_world.get(&coords) {
                        if let Some(jj) = neighbor.index {
                            np[[ii, jj]] = 3;
                        } else {
                            panic!("neighbor with no index");
                        }
                    } else {
                        panic!("missing neighbor at index");
                    }
                }
            }
            if max_jump >= 2 {
                for coords in &world.neighbors2 {
                    if let Some(neighbor) = coords_to_world.get(&coords) {
                        if let Some(jj) = neighbor.index {
                            np[[ii, jj]] = 2;
                        } else {
                            panic!("neighbor with no index");
                        }
                    } else {
                        panic!("missing neighbor at index");
                    }
                }
            }
            if max_jump >= 1 {
                for coords in &world.neighbors1 {
                    if let Some(neighbor) = coords_to_world.get(&coords) {
                        if let Some(jj) = neighbor.index {
                            np[[ii, jj]] = 1;
                        } else {
                            panic!("neighbor with no index");
                        }
                    } else {
                        panic!("missing neighbor at index");
                    }
                }
            }
            for coords in &world.xboat_routes {
                if let Some(neighbor) = coords_to_world.get(&coords) {
                    if let Some(jj) = neighbor.index {
                        np[[ii, jj]] = world.straight_line_distance(neighbor) as i64;
                    } else {
                        panic!("neighbor with no index");
                    }
                } else {
                    panic!("missing neighbor at index");
                }
            }
        } else {
            panic!("Failed to get world");
        }
    }

    let pred = dijkstra(&mut np);
    return (np, pred);
}

/// Absolute coordinates
/// x is an integer
/// y2 is an integer, equal to 2 * y
/// This is needed because y is sometimes a float and floats can't be hash keys
#[derive(Copy, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct Coords {
    x: i64,
    y2: i64,
}

impl Coords {
    fn new(xf: f64, yf: f64) -> Coords {
        let x = xf as i64;
        let y2 = (yf * 2.0) as i64;
        Coords { x, y2 }
    }
}

impl From<Coords> for (f64, f64) {
    fn from(coords: Coords) -> (f64, f64) {
        let fx = coords.x as f64;
        let fy = coords.y2 as f64 / 2.0;
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
    importance: i64,
    economic: String,
    cultural: String,
    nobles: String,
    bases: HashSet<String>,
    zone: String,
    pbg: String,
    worlds: u64,
    allegiance: String,
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
    index: Option<usize>,
}

impl World {
    fn new(
        line: String,
        fields: &Vec<(usize, usize, String)>,
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
        let mut allegiance = "".to_string();
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

        let mut iter = line.chars().enumerate();
        for (start, end, field) in fields.iter() {
            // This intricate loop is to handle the occasional multi-byte
            // UTF-8 character like in Khiinra Ash/Core
            let mut value: String = "".to_string();
            loop {
                let tup_opt: Option<(usize, char)> = iter.next();
                if let Some((ii, ch)) = tup_opt {
                    if ii >= *start && ii < *end {
                        value.push(ch);
                    } else if ii >= *end {
                        break;
                    }
                } else {
                    // end of line
                    break;
                }
            }

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
                "A" => allegiance = value.to_string(),
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
            allegiance,
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

    fn size(&self) -> String {
        return self.uwp.substring(1, 2).to_string();
    }

    fn atmosphere(&self) -> String {
        return self.uwp.substring(2, 3).to_string();
    }

    fn hydrosphere(&self) -> String {
        return self.uwp.substring(3, 4).to_string();
    }

    fn population(&self) -> String {
        return self.uwp.substring(4, 5).to_string();
    }

    fn government(&self) -> String {
        return self.uwp.substring(5, 6).to_string();
    }

    fn law_level(&self) -> String {
        return self.uwp.substring(6, 7).to_string();
    }

    fn tech_level(&self) -> String {
        return self.uwp.substring(8, 9).to_string();
    }

    fn g_tech_level(&self) -> u64 {
        let tech_level_string = self.tech_level();
        let mut tech_level_int = 0;
        for ch in tech_level_string.chars() {
            tech_level_int = ch.to_digit(MAX_TECH_LEVEL + 1).unwrap();
            break;
        }
        return *TECH_LEVEL_TRAVELLER_TO_GURPS.get(&tech_level_int).unwrap();
    }

    fn gas_giants(&self) -> String {
        return self.pbg.substring(2, 3).to_string();
    }

    fn can_refuel(&self) -> bool {
        return self.gas_giants() != "0"
            || (self.zone != "R"
                && ((self.starport() != "E" && self.starport() != "X")
                    || self.hydrosphere() != "0"));
    }

    fn uwtn(&self) -> f64 {
        let gt3 = self.g_tech_level() / 3;
        let tl_mod = gt3 as f64 / 2.0 - 0.5;
        let mut population_int = 0;
        for ch in self.population().chars() {
            population_int = ch.to_digit(MAX_POPULATION + 1).unwrap();
            break;
        }
        let pop_mod = population_int as f64 / 2.0;
        return tl_mod + pop_mod as f64;
    }

    fn wtn_port_modifier(&self) -> f64 {
        let iuwtn = u64::max(0, self.uwtn() as u64);
        return *WTN_PORT_MODIFIER_TABLE
            .get(&(iuwtn, self.g_starport()))
            .unwrap();
    }

    fn wtn(&self) -> f64 {
        return self.uwtn() + self.wtn_port_modifier();
    }

    fn get_coords(&self) -> Coords {
        let hex = &self.hex;
        let location = self.sector_location;
        let x: i64 = hex.substring(0, 2).parse::<i64>().unwrap() + 32 * location.0;
        let y: i64 = hex.substring(2, 4).parse::<i64>().unwrap() + 40 * location.1;
        let mut y2 = 2 * y;
        if x & 1 == 0 {
            y2 += 1;
        }
        return Coords { x, y2 };
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

    fn navigable_distance(&self, other: &World, dist: &Array2<i64>) -> i64 {
        if let Some(ii) = self.index {
            if let Some(jj) = other.index {
                return dist[[ii, jj]];
            } else {
                panic!("navigable_distance without index set");
            }
        } else {
            panic!("navigable_distance without index set");
        }
    }

    fn navigable_path(
        &self,
        other: &World,
        sorted_coords: &Vec<Coords>,
        coords_to_index: &HashMap<Coords, usize>,
        dist: &Array2<i64>,
        pred: &Array2<i64>,
    ) -> Option<Vec<Coords>> {
        if self == other {
            return Some(vec![self.get_coords()]);
        }
        if self.navigable_distance(other, dist) == INFINITY {
            return None;
        }
        let mut path = vec![self.get_coords()];
        let mut coords2 = self.get_coords();
        loop {
            if let Some(ii) = other.index {
                if let Some(jj) = coords_to_index.get(&coords2) {
                    let index = pred[[ii, *jj]];
                    coords2 = sorted_coords[index as usize].clone();
                    if coords2 == other.get_coords() {
                        path.push(coords2);
                        break;
                    } else {
                        path.push(coords2);
                    }
                } else {
                    panic!("navigable_path without index set");
                }
            } else {
                panic!("navigable_path without index set");
            }
        }
        return Some(path);
    }
}

impl Hash for World {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.hex.hash(state);
        self.name.hash(state);
    }
}

impl Ord for World {
    fn cmp(&self, other: &Self) -> Ordering {
        self.get_coords().cmp(&other.get_coords())
    }
}

impl PartialOrd for World {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.get_coords().partial_cmp(&other.get_coords())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Sector {
    names: Vec<String>,
    abbreviation: String,
    location: (i64, i64),
    subsector_letter_to_name: HashMap<String, String>,
    allegiance_code_to_name: HashMap<String, String>,
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
        let allegiance_code_to_name = HashMap::new();
        let hex_to_coords = HashMap::new();
        let mut sector = Sector {
            names,
            abbreviation,
            location,
            subsector_letter_to_name,
            allegiance_code_to_name,
            hex_to_coords,
        };

        sector.parse_xml_metadata(&data_dir, &sector_name).unwrap();
        sector
            .parse_column_data(&data_dir, &sector_name, coords_to_world)
            .unwrap();
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
        let y_opt = root.find("Y");
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
            let allegiance_elements = allegiances_element.find_all("Allegiance");
            for allegiance_element in allegiance_elements {
                let code_opt = allegiance_element.get_attr("Code");
                if let Some(code) = code_opt {
                    let allegiance_name = allegiance_element.text().to_string();
                    if allegiance_name.len() > 0 {
                        self.allegiance_code_to_name
                            .insert(code.to_string(), allegiance_name);
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
        // We initialize fields here to make rustc happy, then overwrite it.
        let mut fields: Vec<(usize, usize, String)> = Vec::new();
        for line in blob.lines() {
            if line.len() == 0 || line.starts_with("#") {
                continue;
            }
            if line.starts_with("Hex") {
                header = line;
            } else if line.starts_with("----") {
                let separator = line;
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
                        let mut start_offset_x = 0;
                        if let Some(start_offset_x2) = start_offset_x_opt {
                            start_offset_x = start_offset_x2.parse()?;
                        };
                        let start_offset_y_opt = route_element.get_attr("StartOffsetY");
                        let mut start_offset_y = 0;
                        if let Some(start_offset_y2) = start_offset_y_opt {
                            start_offset_y = start_offset_y2.parse()?;
                        }
                        let end_offset_x_opt = route_element.get_attr("EndOffsetX");
                        let mut end_offset_x = 0;
                        if let Some(end_offset_x2) = end_offset_x_opt {
                            end_offset_x = end_offset_x2.parse()?;
                        }
                        let end_offset_y_opt = route_element.get_attr("EndOffsetY");
                        let mut end_offset_y = 0;
                        if let Some(end_offset_y2) = end_offset_y_opt {
                            end_offset_y = end_offset_y2.parse()?;
                        }
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
                                        if let Some(_start_world) =
                                            coords_to_world.get(start_coords)
                                        {
                                            if let Some(end_world) =
                                                coords_to_world.get_mut(end_coords)
                                            {
                                                end_world.xboat_routes.insert(*start_coords);
                                            }
                                        }
                                        if let Some(_end_world) = coords_to_world.get(end_coords) {
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

    fn hex_to_world<'a>(
        &'a self,
        hex: String,
        coords_to_world: &'a HashMap<Coords, World>,
    ) -> Option<&World> {
        let coords_opt = self.hex_to_coords.get(&hex);
        if let Some(coords) = coords_opt {
            return coords_to_world.get(coords);
        }
        None
    }
}

fn main() -> Result<()> {
    let args = Args::parse();
    let output_dir = args.output_directory;
    let temp_dir = tempdir()?;
    let mut data_dir: PathBuf = temp_dir.path().to_path_buf();
    if let Some(data_dir_override) = args.data_directory {
        data_dir = data_dir_override;
    };
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
        sector
            .parse_xml_routes(&data_dir, &location_to_sector, &mut coords_to_world)
            .unwrap();
    }
    {
        // Make a temporary clone to avoid having mutable and immutable refs.
        let coords_to_world2 = coords_to_world.clone();
        for world in coords_to_world.values_mut() {
            world.populate_neighbors(&coords_to_world2);
        }
    }
    let mut sorted_coords: Vec<Coords>;
    sorted_coords = coords_to_world.keys().cloned().collect();
    sorted_coords.sort();
    let mut coords_to_index: HashMap<Coords, usize> = HashMap::new();
    for (ii, coords) in sorted_coords.iter_mut().enumerate() {
        coords_to_index.insert(coords.clone(), ii);
        let world_opt = coords_to_world.get_mut(coords);
        if let Some(world) = world_opt {
            world.index = Some(ii);
        } else {
            panic!("World not found at expected coords");
        }
    }
    let (dist2, pred2) = populate_navigable_distances(&sorted_coords, &coords_to_world, 2);
    let (dist3, pred3) = populate_navigable_distances(&sorted_coords, &coords_to_world, 3);

    // TODO

    temp_dir.close()?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::*;
    use std::ffi::OsString;
    use std::fs::read_dir;
    use std::io;

    // Reuse a test directory and downloaded files to avoid overloading travellermap.com
    const TEST_DATA_DIR: &'static str = "/var/tmp/traderust_tests";

    #[fixture]
    #[once]
    fn data_dir() -> PathBuf {
        let data_dir = PathBuf::from(TEST_DATA_DIR);
        create_dir_all(&data_dir).unwrap();
        data_dir
    }

    #[fixture]
    #[once]
    fn download(data_dir: &PathBuf) -> Result<Vec<String>> {
        let sector_names = vec![
            "Deneb".to_string(),
            "Gvurrdon".to_string(),
            "Spinward Marches".to_string(),
            "Core".to_string(),
        ];
        download_sector_data(&data_dir, &sector_names)?;

        Ok(sector_names)
    }

    #[rstest]
    fn test_coords() {
        let mut x = -101.0;
        let mut y = -101.0;
        while x <= 101.0 {
            while y <= 101.0 {
                let coords = Coords::new(x, y);
                let (x2, y2) = <(f64, f64)>::from(coords);
                assert_eq!(x, x2);
                assert_eq!(y, y2);
                y += 0.5;
            }
            x += 1.0;
        }
    }

    #[rstest]
    fn test_download_sector_data(data_dir: &PathBuf, download: &Result<Vec<String>>) -> Result<()> {
        let mut expected_filenames = Vec::new();
        if let Ok(sector_names) = download {
            for sector_name in sector_names {
                expected_filenames.push(sector_name.to_owned() + ".sec");
                expected_filenames.push(sector_name.to_owned() + ".xml");
            }
            expected_filenames.sort();
        }

        let found_filename_results: Vec<Result<OsString, io::Error>> = read_dir(&data_dir)?
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

        Ok(())
    }

    #[rstest]
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
        assert_eq!(fields[0], (0, 4, "Hex".to_string()));
        assert_eq!(fields[1], (5, 25, "Name".to_string()));
        assert_eq!(fields[2], (26, 35, "UWP".to_string()));
        assert_eq!(fields[3], (36, 76, "Remarks".to_string()));
        assert_eq!(fields[4], (77, 83, "{Ix}".to_string()));
        assert_eq!(fields[5], (84, 91, "(Ex)".to_string()));
        assert_eq!(fields[6], (92, 98, "[Cx]".to_string()));
        assert_eq!(fields[7], (99, 104, "N".to_string()));
        assert_eq!(fields[8], (105, 107, "B".to_string()));
        assert_eq!(fields[9], (108, 109, "Z".to_string()));
        assert_eq!(fields[10], (110, 113, "PBG".to_string()));
        assert_eq!(fields[11], (114, 116, "W".to_string()));
        assert_eq!(fields[12], (117, 121, "A".to_string()));
        assert_eq!(fields[13], (122, 136, "Stellar".to_string()));

        Ok(())
    }

    #[rstest]
    fn test_sector_spin(data_dir: &PathBuf, download: &Result<Vec<String>>) -> Result<()> {
        if let Ok(_sector_names) = download {};
        let sector_name = "Spinward Marches".to_string();
        let mut coords_to_world: HashMap<Coords, World> = HashMap::new();
        let sector = Sector::new(&data_dir, sector_name, &mut coords_to_world);

        assert_eq!(sector.name(), "Spinward Marches");
        assert_eq!(sector.names, vec!["Spinward Marches", "Tloql"]);
        assert_eq!(sector.abbreviation, "Spin");
        assert_eq!(sector.location, (-4, -1));
        assert_eq!(sector.subsector_letter_to_name.len(), 16);
        assert_eq!(
            *sector.subsector_letter_to_name.get("A").unwrap(),
            "Cronor".to_string()
        );
        assert_eq!(
            *sector.subsector_letter_to_name.get("P").unwrap(),
            "Trin's Veil".to_string()
        );
        assert_eq!(sector.allegiance_code_to_name.len(), 8);
        assert_eq!(
            *sector.allegiance_code_to_name.get("CsIm").unwrap(),
            "Client state, Third Imperium".to_string()
        );
        assert_eq!(sector.hex_to_coords.len(), 439);
        let zeycude_coords = sector.hex_to_coords.get("0101").unwrap();
        let zeycude = coords_to_world.get(zeycude_coords).unwrap();
        assert_eq!(zeycude.name, "Zeycude");
        let hazel_coords = sector.hex_to_coords.get("3236").unwrap();
        let hazel = coords_to_world.get(hazel_coords).unwrap();
        assert_eq!(hazel.name, "Hazel");

        Ok(())
    }

    #[rstest]
    fn test_sector_dene(data_dir: &PathBuf, download: &Result<Vec<String>>) -> Result<()> {
        if let Ok(_sector_names) = download {};
        let sector_name = "Deneb".to_string();
        let mut coords_to_world: HashMap<Coords, World> = HashMap::new();
        let sector = Sector::new(&data_dir, sector_name, &mut coords_to_world);

        assert_eq!(sector.name(), "Deneb");
        assert_eq!(sector.names, vec!["Deneb", "Nieklsdia"]);
        assert_eq!(sector.abbreviation, "Dene");
        assert_eq!(sector.location, (-3, -1));
        assert_eq!(sector.subsector_letter_to_name.len(), 16);
        assert_eq!(
            *sector.subsector_letter_to_name.get("A").unwrap(),
            "Pretoria".to_string()
        );
        assert_eq!(
            *sector.subsector_letter_to_name.get("P").unwrap(),
            "Vast Heavens".to_string()
        );
        assert_eq!(sector.allegiance_code_to_name.len(), 6);
        assert_eq!(
            *sector.allegiance_code_to_name.get("CsIm").unwrap(),
            "Client state, Third Imperium".to_string()
        );
        assert_eq!(sector.hex_to_coords.len(), 386);
        let new_ramma_coords = sector.hex_to_coords.get("0108").unwrap();
        let new_ramma = coords_to_world.get(new_ramma_coords).unwrap();
        assert_eq!(new_ramma.name, "New Ramma");
        let asharam_coords = sector.hex_to_coords.get("3031").unwrap();
        let asharam = coords_to_world.get(asharam_coords).unwrap();
        assert_eq!(asharam.name, "Asharam");

        Ok(())
    }

    #[rstest]
    fn test_sector_gvur(data_dir: &PathBuf, download: &Result<Vec<String>>) -> Result<()> {
        if let Ok(_sector_names) = download {};
        let sector_name = "Gvurrdon".to_string();
        let mut coords_to_world: HashMap<Coords, World> = HashMap::new();
        let sector = Sector::new(&data_dir, sector_name, &mut coords_to_world);

        assert_eq!(sector.name(), "Gvurrdon");
        assert_eq!(sector.names, vec!["Gvurrdon", r"Briakqra'"]);
        assert_eq!(sector.abbreviation, "Gvur");
        assert_eq!(sector.location, (-4, -2));
        assert_eq!(sector.subsector_letter_to_name.len(), 16);
        assert_eq!(
            *sector.subsector_letter_to_name.get("A").unwrap(),
            "Ongvos".to_string()
        );
        assert_eq!(
            *sector.subsector_letter_to_name.get("P").unwrap(),
            "Firgr".to_string()
        );
        assert_eq!(sector.allegiance_code_to_name.len(), 16);
        assert_eq!(
            *sector.allegiance_code_to_name.get("CsIm").unwrap(),
            "Client state, Third Imperium".to_string()
        );
        assert_eq!(sector.hex_to_coords.len(), 358);
        let enjtodl_coords = sector.hex_to_coords.get("0104").unwrap();
        let enjtodl = coords_to_world.get(enjtodl_coords).unwrap();
        assert_eq!(enjtodl.name, "Enjtodl");
        let oertsous_coords = sector.hex_to_coords.get("3238").unwrap();
        let oertsous = coords_to_world.get(oertsous_coords).unwrap();
        assert_eq!(oertsous.name, "Oertsous");

        Ok(())
    }

    #[rstest]
    fn test_world_aramis(data_dir: &PathBuf, download: &Result<Vec<String>>) -> Result<()> {
        if let Ok(_sector_names) = download {};
        let sector_name = "Spinward Marches".to_string();
        let mut coords_to_world: HashMap<Coords, World> = HashMap::new();
        let sector = Sector::new(&data_dir, sector_name, &mut coords_to_world);

        let aramis_coords = sector.hex_to_coords.get("3110").unwrap();
        let aramis = coords_to_world.get(aramis_coords).unwrap();
        assert_eq!(aramis.name, "Aramis");
        assert_eq!(aramis.sector_location, (-4, -1));
        assert_eq!(aramis.hex, "3110");
        assert_eq!(aramis.uwp, "A5A0556-B");

        let mut tc = HashSet::new();
        tc.insert("He".to_string());
        tc.insert("Ni".to_string());
        tc.insert("Cp".to_string());
        assert_eq!(aramis.trade_classifications, tc);

        assert_eq!(aramis.importance, 2);
        assert_eq!(aramis.economic, "846+1");
        assert_eq!(aramis.cultural, "474A");
        assert_eq!(aramis.nobles, "BF");
        let mut bases = HashSet::new();
        bases.insert("N".to_string());
        bases.insert("S".to_string());
        assert_eq!(aramis.bases, bases);
        assert_eq!(aramis.zone, "G");
        assert_eq!(aramis.pbg, "710");
        assert_eq!(aramis.worlds, 9);
        assert_eq!(aramis.allegiance, "ImDd");
        assert_eq!(aramis.stars, vec!["M2 V"]);
        assert_eq!(aramis.starport(), "A");
        assert_eq!(aramis.g_starport(), "V");
        assert_eq!(aramis.size(), "5");
        assert_eq!(aramis.atmosphere(), "A");
        assert_eq!(aramis.hydrosphere(), "0");
        assert_eq!(aramis.population(), "5");
        assert_eq!(aramis.government(), "5");
        assert_eq!(aramis.law_level(), "6");
        assert_eq!(aramis.tech_level(), "B");
        assert_eq!(aramis.g_tech_level(), 9);
        assert_eq!(aramis.uwtn(), 3.5);
        assert_eq!(aramis.wtn_port_modifier(), 0.5);
        assert_eq!(aramis.wtn(), 4.0);
        assert_eq!(aramis.gas_giants(), "0");
        assert!(aramis.can_refuel());

        Ok(())
    }

    #[rstest]
    fn test_world_regina(data_dir: &PathBuf, download: &Result<Vec<String>>) -> Result<()> {
        if let Ok(_sector_names) = download {};
        let sector_name = "Spinward Marches".to_string();
        let mut coords_to_world: HashMap<Coords, World> = HashMap::new();
        let sector = Sector::new(&data_dir, sector_name, &mut coords_to_world);

        let regina_coords = sector.hex_to_coords.get("1910").unwrap();
        let regina = coords_to_world.get(regina_coords).unwrap();
        assert_eq!(regina.name, "Regina");
        assert_eq!(regina.sector_location, (-4, -1));
        assert_eq!(regina.hex, "1910");
        assert_eq!(regina.uwp, "A788899-C");

        let mut tc = HashSet::new();
        tc.insert("Ri".to_string());
        tc.insert("Pa".to_string());
        tc.insert("Ph".to_string());
        tc.insert("An".to_string());
        tc.insert("Cp".to_string());
        tc.insert("(Amindii)2".to_string());
        tc.insert("Varg0".to_string());
        tc.insert("Asla0".to_string());
        tc.insert("Sa".to_string());
        assert_eq!(regina.trade_classifications, tc);

        assert_eq!(regina.importance, 4);
        assert_eq!(regina.economic, "D7E+5");
        assert_eq!(regina.cultural, "9C6D");
        assert_eq!(regina.nobles, "BcCeF");
        let mut bases = HashSet::new();
        bases.insert("N".to_string());
        bases.insert("S".to_string());
        assert_eq!(regina.bases, bases);
        assert_eq!(regina.zone, "G");
        assert_eq!(regina.pbg, "703");
        assert_eq!(regina.worlds, 8);
        assert_eq!(regina.allegiance, "ImDd");
        assert_eq!(regina.stars, vec!["F7 V", "BD", "M3 V"]);
        assert_eq!(regina.starport(), "A");
        assert_eq!(regina.g_starport(), "V");
        assert_eq!(regina.size(), "7");
        assert_eq!(regina.atmosphere(), "8");
        assert_eq!(regina.hydrosphere(), "8");
        assert_eq!(regina.population(), "8");
        assert_eq!(regina.government(), "9");
        assert_eq!(regina.law_level(), "9");
        assert_eq!(regina.tech_level(), "C");
        assert_eq!(regina.g_tech_level(), 10);
        assert_eq!(regina.uwtn(), 5.0);
        assert_eq!(regina.wtn_port_modifier(), 0.0);
        assert_eq!(regina.wtn(), 5.0);
        assert_eq!(regina.gas_giants(), "3");
        assert!(regina.can_refuel());

        Ok(())
    }

    #[rstest]
    fn test_world_bronze(data_dir: &PathBuf, download: &Result<Vec<String>>) -> Result<()> {
        if let Ok(_sector_names) = download {};
        let sector_name = "Spinward Marches".to_string();
        let mut coords_to_world: HashMap<Coords, World> = HashMap::new();
        let sector = Sector::new(&data_dir, sector_name, &mut coords_to_world);

        let bronze_coords = sector.hex_to_coords.get("1627").unwrap();
        let bronze = coords_to_world.get(bronze_coords).unwrap();
        assert_eq!(bronze.name, "Bronze");
        assert_eq!(bronze.sector_location, (-4, -1));
        assert_eq!(bronze.hex, "1627");
        assert_eq!(bronze.uwp, "E201000-0");

        let mut tc = HashSet::new();
        tc.insert("Ba".to_string());
        tc.insert("Ic".to_string());
        tc.insert("Re".to_string());
        tc.insert("Va".to_string());
        assert_eq!(bronze.trade_classifications, tc);

        assert_eq!(bronze.importance, -3);
        assert_eq!(bronze.economic, "200-5");
        assert_eq!(bronze.cultural, "0000");
        assert_eq!(bronze.nobles, "");
        let bases = HashSet::new();
        assert_eq!(bronze.bases, bases);
        assert_eq!(bronze.zone, "G");
        assert_eq!(bronze.pbg, "010");
        assert_eq!(bronze.worlds, 5);
        assert_eq!(bronze.allegiance, "SwCf");
        assert_eq!(bronze.stars, vec!["M3 V"]);
        assert_eq!(bronze.starport(), "E");
        assert_eq!(bronze.g_starport(), "I");
        assert_eq!(bronze.size(), "2");
        assert_eq!(bronze.atmosphere(), "0");
        assert_eq!(bronze.hydrosphere(), "1");
        assert_eq!(bronze.population(), "0");
        assert_eq!(bronze.government(), "0");
        assert_eq!(bronze.law_level(), "0");
        assert_eq!(bronze.tech_level(), "0");
        assert_eq!(bronze.g_tech_level(), 2);
        assert_eq!(bronze.uwtn(), -0.5);
        assert_eq!(bronze.wtn_port_modifier(), 0.5);
        assert_eq!(bronze.wtn(), 0.0);
        assert_eq!(bronze.gas_giants(), "0");
        assert!(bronze.can_refuel());

        Ok(())
    }

    #[rstest]
    fn test_world_callia(data_dir: &PathBuf, download: &Result<Vec<String>>) -> Result<()> {
        if let Ok(_sector_names) = download {};
        let sector_name = "Spinward Marches".to_string();
        let mut coords_to_world: HashMap<Coords, World> = HashMap::new();
        let sector = Sector::new(&data_dir, sector_name, &mut coords_to_world);

        let callia_coords = sector.hex_to_coords.get("1836").unwrap();
        let callia = coords_to_world.get(callia_coords).unwrap();
        assert_eq!(callia.name, "Callia");
        assert_eq!(callia.sector_location, (-4, -1));
        assert_eq!(callia.hex, "1836");
        assert_eq!(callia.uwp, "E550852-6");

        let mut tc = HashSet::new();
        tc.insert("De".to_string());
        tc.insert("Po".to_string());
        tc.insert("Ph".to_string());
        assert_eq!(callia.trade_classifications, tc);

        assert_eq!(callia.importance, -2);
        assert_eq!(callia.economic, "A75-5");
        assert_eq!(callia.cultural, "4612");
        assert_eq!(callia.nobles, "Be");
        let bases = HashSet::new();
        assert_eq!(callia.bases, bases);
        assert_eq!(callia.zone, "G");
        assert_eq!(callia.pbg, "810");
        assert_eq!(callia.worlds, 11);
        assert_eq!(callia.allegiance, "ImDd");
        assert_eq!(callia.stars, vec!["M3 V"]);
        assert_eq!(callia.starport(), "E");
        assert_eq!(callia.g_starport(), "I");
        assert_eq!(callia.size(), "5");
        assert_eq!(callia.atmosphere(), "5");
        assert_eq!(callia.hydrosphere(), "0");
        assert_eq!(callia.population(), "8");
        assert_eq!(callia.government(), "5");
        assert_eq!(callia.law_level(), "2");
        assert_eq!(callia.tech_level(), "6");
        assert_eq!(callia.g_tech_level(), 6);
        assert_eq!(callia.uwtn(), 4.5);
        assert_eq!(callia.wtn_port_modifier(), -1.0);
        assert_eq!(callia.wtn(), 3.5);
        assert_eq!(callia.gas_giants(), "0");
        assert!(!callia.can_refuel());

        Ok(())
    }

    #[rstest]
    fn test_world_candory(data_dir: &PathBuf, download: &Result<Vec<String>>) -> Result<()> {
        if let Ok(_sector_names) = download {};
        let sector_name = "Spinward Marches".to_string();
        let mut coords_to_world: HashMap<Coords, World> = HashMap::new();
        let sector = Sector::new(&data_dir, sector_name, &mut coords_to_world);

        let candory_coords = sector.hex_to_coords.get("0336").unwrap();
        let candory = coords_to_world.get(candory_coords).unwrap();
        assert_eq!(candory.name, "Candory");
        assert_eq!(candory.sector_location, (-4, -1));
        assert_eq!(candory.hex, "0336");
        assert_eq!(candory.uwp, "C593634-8");

        let mut tc = HashSet::new();
        tc.insert("Ni".to_string());
        tc.insert("An".to_string());
        tc.insert("Fo".to_string());
        tc.insert("DroyW".to_string());
        assert_eq!(candory.trade_classifications, tc);

        assert_eq!(candory.importance, -2);
        assert_eq!(candory.economic, "A52-4");
        assert_eq!(candory.cultural, "4436");
        assert_eq!(candory.nobles, "");
        let bases = HashSet::new();
        assert_eq!(candory.bases, bases);
        assert_eq!(candory.zone, "R");
        assert_eq!(candory.pbg, "920");
        assert_eq!(candory.worlds, 5);
        assert_eq!(candory.allegiance, "ImDd");
        assert_eq!(candory.stars, vec!["F6 V", "M3 V"]);
        assert_eq!(candory.starport(), "C");
        assert_eq!(candory.g_starport(), "III");
        assert_eq!(candory.size(), "5");
        assert_eq!(candory.atmosphere(), "9");
        assert_eq!(candory.hydrosphere(), "3");
        assert_eq!(candory.population(), "6");
        assert_eq!(candory.government(), "3");
        assert_eq!(candory.law_level(), "4");
        assert_eq!(candory.tech_level(), "8");
        assert_eq!(candory.g_tech_level(), 8);
        assert_eq!(candory.uwtn(), 3.5);
        assert_eq!(candory.wtn_port_modifier(), 0.0);
        assert_eq!(candory.wtn(), 3.5);
        assert_eq!(candory.gas_giants(), "0");
        assert!(!candory.can_refuel());

        Ok(())
    }

    #[rstest]
    fn test_world_khiinra_ash(data_dir: &PathBuf, download: &Result<Vec<String>>) -> Result<()> {
        if let Ok(_sector_names) = download {};
        let sector_name = "Core".to_string();
        let mut coords_to_world: HashMap<Coords, World> = HashMap::new();
        let sector = Sector::new(&data_dir, sector_name, &mut coords_to_world);

        let khiinra_ash_coords = sector.hex_to_coords.get("2916").unwrap();
        let khiinra_ash = coords_to_world.get(khiinra_ash_coords).unwrap();
        assert_eq!(khiinra_ash.name, "Khiinra Ash");
        assert_eq!(khiinra_ash.sector_location, (0, 0));
        assert_eq!(khiinra_ash.hex, "2916");
        assert_eq!(khiinra_ash.uwp, "BAE6362-8");
        // No test for trade classifications to avoid UTF-8 in the code
        assert_eq!(khiinra_ash.importance, -1);
        assert_eq!(khiinra_ash.economic, "920-5");
        assert_eq!(khiinra_ash.cultural, "1214");
        assert_eq!(khiinra_ash.nobles, "B");
        let bases = HashSet::new();
        assert_eq!(khiinra_ash.bases, bases);
        assert_eq!(khiinra_ash.zone, "G");
        assert_eq!(khiinra_ash.pbg, "704");
        assert_eq!(khiinra_ash.worlds, 7);
        assert_eq!(khiinra_ash.allegiance, "ImSy");
        assert_eq!(khiinra_ash.stars, vec!["M1 V", "M2 V"]);
        assert_eq!(khiinra_ash.starport(), "B");
        assert_eq!(khiinra_ash.g_starport(), "IV");
        assert_eq!(khiinra_ash.size(), "A");
        assert_eq!(khiinra_ash.atmosphere(), "E");
        assert_eq!(khiinra_ash.hydrosphere(), "6");
        assert_eq!(khiinra_ash.population(), "3");
        assert_eq!(khiinra_ash.government(), "6");
        assert_eq!(khiinra_ash.law_level(), "2");
        assert_eq!(khiinra_ash.tech_level(), "8");
        assert_eq!(khiinra_ash.g_tech_level(), 8);
        assert_eq!(khiinra_ash.uwtn(), 2.0);
        assert_eq!(khiinra_ash.wtn_port_modifier(), 0.5);
        assert_eq!(khiinra_ash.wtn(), 2.5);
        assert_eq!(khiinra_ash.gas_giants(), "4");
        assert!(khiinra_ash.can_refuel());

        Ok(())
    }

    #[rstest]
    fn test_abs_coords(data_dir: &PathBuf, download: &Result<Vec<String>>) -> Result<()> {
        if let Ok(_sector_names) = download {};
        let mut coords_to_world: HashMap<Coords, World> = HashMap::new();
        let spin = Sector::new(
            &data_dir,
            "Spinward Marches".to_string(),
            &mut coords_to_world,
        );
        let dene = Sector::new(&data_dir, "Deneb".to_string(), &mut coords_to_world);

        let aramis = spin
            .hex_to_world("3110".to_string(), &coords_to_world)
            .unwrap();
        let ldd = spin
            .hex_to_world("3010".to_string(), &coords_to_world)
            .unwrap();
        let natoko = spin
            .hex_to_world("3209".to_string(), &coords_to_world)
            .unwrap();
        let reacher = spin
            .hex_to_world("3210".to_string(), &coords_to_world)
            .unwrap();
        let vinorian = spin
            .hex_to_world("3111".to_string(), &coords_to_world)
            .unwrap();
        let nutema = spin
            .hex_to_world("3112".to_string(), &coords_to_world)
            .unwrap();
        let margesi = spin
            .hex_to_world("3212".to_string(), &coords_to_world)
            .unwrap();
        let saarinen = dene
            .hex_to_world("0113".to_string(), &coords_to_world)
            .unwrap();
        let regina = spin
            .hex_to_world("1910".to_string(), &coords_to_world)
            .unwrap();

        assert_eq!(<(f64, f64)>::from(aramis.get_coords()), (-97.0, -30.0));
        assert_eq!(<(f64, f64)>::from(ldd.get_coords()), (-98.0, -29.5));
        assert_eq!(<(f64, f64)>::from(natoko.get_coords()), (-96.0, -30.5));
        assert_eq!(<(f64, f64)>::from(reacher.get_coords()), (-96.0, -29.5));
        assert_eq!(<(f64, f64)>::from(vinorian.get_coords()), (-97.0, -29.0));
        assert_eq!(<(f64, f64)>::from(nutema.get_coords()), (-97.0, -28.0));
        assert_eq!(<(f64, f64)>::from(margesi.get_coords()), (-96.0, -27.5));
        assert_eq!(<(f64, f64)>::from(saarinen.get_coords()), (-95.0, -27.0));
        assert_eq!(<(f64, f64)>::from(regina.get_coords()), (-109.0, -30.0));

        Ok(())
    }

    #[rstest]
    fn test_straight_line_distance(
        data_dir: &PathBuf,
        download: &Result<Vec<String>>,
    ) -> Result<()> {
        if let Ok(_sector_names) = download {};
        let mut coords_to_world: HashMap<Coords, World> = HashMap::new();
        let spin = Sector::new(
            &data_dir,
            "Spinward Marches".to_string(),
            &mut coords_to_world,
        );
        let dene = Sector::new(&data_dir, "Deneb".to_string(), &mut coords_to_world);

        let aramis = spin
            .hex_to_world("3110".to_string(), &coords_to_world)
            .unwrap();
        let ldd = spin
            .hex_to_world("3010".to_string(), &coords_to_world)
            .unwrap();
        let natoko = spin
            .hex_to_world("3209".to_string(), &coords_to_world)
            .unwrap();
        let reacher = spin
            .hex_to_world("3210".to_string(), &coords_to_world)
            .unwrap();
        let vinorian = spin
            .hex_to_world("3111".to_string(), &coords_to_world)
            .unwrap();
        let nutema = spin
            .hex_to_world("3112".to_string(), &coords_to_world)
            .unwrap();
        let margesi = spin
            .hex_to_world("3212".to_string(), &coords_to_world)
            .unwrap();
        let patinir = spin
            .hex_to_world("3207".to_string(), &coords_to_world)
            .unwrap();
        let saarinen = dene
            .hex_to_world("0113".to_string(), &coords_to_world)
            .unwrap();
        let regina = spin
            .hex_to_world("1910".to_string(), &coords_to_world)
            .unwrap();
        let corfu = spin
            .hex_to_world("2602".to_string(), &coords_to_world)
            .unwrap();
        let lablon = spin
            .hex_to_world("2701".to_string(), &coords_to_world)
            .unwrap();
        let junidy = spin
            .hex_to_world("3202".to_string(), &coords_to_world)
            .unwrap();
        let marz = dene
            .hex_to_world("0201".to_string(), &coords_to_world)
            .unwrap();

        assert_eq!(aramis.straight_line_distance(aramis), 0);
        assert_eq!(aramis.straight_line_distance(ldd), 1);
        assert_eq!(ldd.straight_line_distance(aramis), 1);
        assert_eq!(aramis.straight_line_distance(natoko), 1);
        assert_eq!(aramis.straight_line_distance(reacher), 1);
        assert_eq!(natoko.straight_line_distance(reacher), 1);
        assert_eq!(aramis.straight_line_distance(vinorian), 1);
        assert_eq!(vinorian.straight_line_distance(nutema), 1);
        assert_eq!(nutema.straight_line_distance(margesi), 1);
        assert_eq!(margesi.straight_line_distance(saarinen), 1);
        assert_eq!(ldd.straight_line_distance(natoko), 2);
        assert_eq!(ldd.straight_line_distance(reacher), 2);
        assert_eq!(ldd.straight_line_distance(nutema), 2);
        assert_eq!(ldd.straight_line_distance(margesi), 3);
        assert_eq!(aramis.straight_line_distance(patinir), 3);
        assert_eq!(ldd.straight_line_distance(saarinen), 4);
        assert_eq!(aramis.straight_line_distance(corfu), 10);
        assert_eq!(aramis.straight_line_distance(lablon), 11);
        assert_eq!(aramis.straight_line_distance(junidy), 8);
        assert_eq!(aramis.straight_line_distance(marz), 10);
        assert_eq!(aramis.straight_line_distance(regina), 12);

        Ok(())
    }

    #[rstest]
    fn test_xboat_routes(data_dir: &PathBuf, download: &Result<Vec<String>>) -> Result<()> {
        if let Ok(_sector_names) = download {};
        let mut coords_to_world: HashMap<Coords, World> = HashMap::new();
        let mut location_to_sector: HashMap<(i64, i64), Sector> = HashMap::new();
        let spin = Sector::new(
            &data_dir,
            "Spinward Marches".to_string(),
            &mut coords_to_world,
        );
        let dene = Sector::new(&data_dir, "Deneb".to_string(), &mut coords_to_world);
        location_to_sector.insert(spin.location, spin.clone());
        location_to_sector.insert(dene.location, dene.clone());
        for sector in location_to_sector.values() {
            sector
                .parse_xml_routes(&data_dir, &location_to_sector, &mut coords_to_world)
                .unwrap();
        }
        let aramis = spin
            .hex_to_world("3110".to_string(), &coords_to_world)
            .unwrap();
        let ldd = spin
            .hex_to_world("3010".to_string(), &coords_to_world)
            .unwrap();
        let natoko = spin
            .hex_to_world("3209".to_string(), &coords_to_world)
            .unwrap();
        let reacher = spin
            .hex_to_world("3210".to_string(), &coords_to_world)
            .unwrap();
        let vinorian = spin
            .hex_to_world("3111".to_string(), &coords_to_world)
            .unwrap();
        let nutema = spin
            .hex_to_world("3112".to_string(), &coords_to_world)
            .unwrap();
        let saarinen = dene
            .hex_to_world("0113".to_string(), &coords_to_world)
            .unwrap();
        let regina = spin
            .hex_to_world("1910".to_string(), &coords_to_world)
            .unwrap();
        let corfu = spin
            .hex_to_world("2602".to_string(), &coords_to_world)
            .unwrap();
        let lablon = spin
            .hex_to_world("2701".to_string(), &coords_to_world)
            .unwrap();
        let junidy = spin
            .hex_to_world("3202".to_string(), &coords_to_world)
            .unwrap();
        let marz = dene
            .hex_to_world("0201".to_string(), &coords_to_world)
            .unwrap();
        let celepina = spin
            .hex_to_world("2913".to_string(), &coords_to_world)
            .unwrap();
        let teh = dene
            .hex_to_world("0208".to_string(), &coords_to_world)
            .unwrap();
        let ash = dene
            .hex_to_world("0504".to_string(), &coords_to_world)
            .unwrap();
        let roup = spin
            .hex_to_world("2007".to_string(), &coords_to_world)
            .unwrap();
        let jenghe = spin
            .hex_to_world("1810".to_string(), &coords_to_world)
            .unwrap();
        let dinomn = spin
            .hex_to_world("1912".to_string(), &coords_to_world)
            .unwrap();
        let towers = spin
            .hex_to_world("3103".to_string(), &coords_to_world)
            .unwrap();

        let mut set = HashSet::new();
        assert_eq!(reacher.xboat_routes, set);
        assert_eq!(vinorian.xboat_routes, set);
        assert_eq!(nutema.xboat_routes, set);
        assert_eq!(saarinen.xboat_routes, set);
        assert_eq!(corfu.xboat_routes, set);
        assert_eq!(lablon.xboat_routes, set);

        set.insert(ldd.get_coords());
        set.insert(natoko.get_coords());
        assert_eq!(aramis.xboat_routes, set);

        set.clear();
        set.insert(aramis.get_coords());
        set.insert(celepina.get_coords());
        assert_eq!(ldd.xboat_routes, set);

        set.clear();
        set.insert(aramis.get_coords());
        set.insert(teh.get_coords());
        assert_eq!(natoko.xboat_routes, set);

        set.clear();
        assert_eq!(reacher.xboat_routes, set);
        assert_eq!(vinorian.xboat_routes, set);
        assert_eq!(nutema.xboat_routes, set);
        assert_eq!(saarinen.xboat_routes, set);
        assert_eq!(corfu.xboat_routes, set);
        assert_eq!(lablon.xboat_routes, set);

        set.clear();
        set.insert(marz.get_coords());
        set.insert(towers.get_coords());
        assert_eq!(junidy.xboat_routes, set);

        set.clear();
        set.insert(junidy.get_coords());
        set.insert(ash.get_coords());
        assert_eq!(marz.xboat_routes, set);

        set.clear();
        set.insert(roup.get_coords());
        set.insert(jenghe.get_coords());
        set.insert(dinomn.get_coords());
        assert_eq!(regina.xboat_routes, set);

        Ok(())
    }

    #[rstest]
    fn test_neighbors(data_dir: &PathBuf, download: &Result<Vec<String>>) -> Result<()> {
        if let Ok(_sector_names) = download {};
        let mut coords_to_world: HashMap<Coords, World> = HashMap::new();
        let mut location_to_sector: HashMap<(i64, i64), Sector> = HashMap::new();
        let spin = Sector::new(
            &data_dir,
            "Spinward Marches".to_string(),
            &mut coords_to_world,
        );
        let dene = Sector::new(&data_dir, "Deneb".to_string(), &mut coords_to_world);
        location_to_sector.insert(spin.location, spin.clone());
        location_to_sector.insert(dene.location, dene.clone());
        for sector in location_to_sector.values() {
            sector
                .parse_xml_routes(&data_dir, &location_to_sector, &mut coords_to_world)
                .unwrap();
        }

        // Make a temporary clone to avoid having mutable and immutable refs.
        let coords_to_world2 = coords_to_world.clone();
        for world in coords_to_world.values_mut() {
            world.populate_neighbors(&coords_to_world2);
        }

        let aramis = spin
            .hex_to_world("3110".to_string(), &coords_to_world)
            .unwrap();
        let ldd = spin
            .hex_to_world("3010".to_string(), &coords_to_world)
            .unwrap();
        let natoko = spin
            .hex_to_world("3209".to_string(), &coords_to_world)
            .unwrap();
        let reacher = spin
            .hex_to_world("3210".to_string(), &coords_to_world)
            .unwrap();
        let vinorian = spin
            .hex_to_world("3111".to_string(), &coords_to_world)
            .unwrap();
        let nutema = spin
            .hex_to_world("3112".to_string(), &coords_to_world)
            .unwrap();
        let teh = dene
            .hex_to_world("0208".to_string(), &coords_to_world)
            .unwrap();
        let pysadi = spin
            .hex_to_world("3008".to_string(), &coords_to_world)
            .unwrap();
        let margesi = spin
            .hex_to_world("3212".to_string(), &coords_to_world)
            .unwrap();
        let zila = spin
            .hex_to_world("2908".to_string(), &coords_to_world)
            .unwrap();
        let lewis = spin
            .hex_to_world("3107".to_string(), &coords_to_world)
            .unwrap();
        let patinir = spin
            .hex_to_world("3207".to_string(), &coords_to_world)
            .unwrap();
        let henoz = spin
            .hex_to_world("2912".to_string(), &coords_to_world)
            .unwrap();
        let suvfoto = dene
            .hex_to_world("0211".to_string(), &coords_to_world)
            .unwrap();
        let kretikaa = dene
            .hex_to_world("0209".to_string(), &coords_to_world)
            .unwrap();
        let new_ramma = dene
            .hex_to_world("0108".to_string(), &coords_to_world)
            .unwrap();
        let valhalla = spin
            .hex_to_world("2811".to_string(), &coords_to_world)
            .unwrap();
        let saarinen = dene
            .hex_to_world("0113".to_string(), &coords_to_world)
            .unwrap();
        let celepina = spin
            .hex_to_world("2913".to_string(), &coords_to_world)
            .unwrap();
        let zivije = spin
            .hex_to_world("2812".to_string(), &coords_to_world)
            .unwrap();

        let mut set = HashSet::new();
        set.insert(ldd.get_coords());
        set.insert(natoko.get_coords());
        set.insert(reacher.get_coords());
        set.insert(vinorian.get_coords());
        assert_eq!(aramis.neighbors1, set);

        set.clear();
        set.insert(nutema.get_coords());
        set.insert(pysadi.get_coords());
        assert_eq!(aramis.neighbors2, set);

        set.clear();
        set.insert(margesi.get_coords());
        set.insert(teh.get_coords());
        set.insert(zila.get_coords());
        set.insert(lewis.get_coords());
        set.insert(patinir.get_coords());
        set.insert(henoz.get_coords());
        set.insert(suvfoto.get_coords());
        set.insert(kretikaa.get_coords());
        set.insert(new_ramma.get_coords());
        set.insert(valhalla.get_coords());
        assert_eq!(aramis.neighbors3, set);

        set.clear();
        set.insert(aramis.get_coords());
        set.insert(ldd.get_coords());
        set.insert(reacher.get_coords());
        set.insert(nutema.get_coords());
        assert_eq!(vinorian.neighbors1, set);

        set.clear();
        set.insert(natoko.get_coords());
        set.insert(margesi.get_coords());
        set.insert(henoz.get_coords());
        assert_eq!(vinorian.neighbors2, set);

        set.clear();
        set.insert(kretikaa.get_coords());
        set.insert(suvfoto.get_coords());
        set.insert(saarinen.get_coords());
        // set.insert(huderu.get_coords()); // Can't refuel
        set.insert(celepina.get_coords());
        set.insert(zivije.get_coords());
        set.insert(valhalla.get_coords());
        set.insert(pysadi.get_coords());
        assert_eq!(vinorian.neighbors3, set);

        Ok(())
    }

    #[rstest]
    fn test_navigable_distance(data_dir: &PathBuf, download: &Result<Vec<String>>) -> Result<()> {
        if let Ok(_sector_names) = download {};
        let mut coords_to_world: HashMap<Coords, World> = HashMap::new();
        let mut location_to_sector: HashMap<(i64, i64), Sector> = HashMap::new();
        let spin = Sector::new(
            &data_dir,
            "Spinward Marches".to_string(),
            &mut coords_to_world,
        );
        let dene = Sector::new(&data_dir, "Deneb".to_string(), &mut coords_to_world);
        location_to_sector.insert(spin.location, spin.clone());
        location_to_sector.insert(dene.location, dene.clone());
        for sector in location_to_sector.values() {
            sector
                .parse_xml_routes(&data_dir, &location_to_sector, &mut coords_to_world)
                .unwrap();
        }
        // Make a temporary clone to avoid having mutable and immutable refs.
        let coords_to_world2 = coords_to_world.clone();
        for world in coords_to_world.values_mut() {
            world.populate_neighbors(&coords_to_world2);
        }
        let mut sorted_coords: Vec<Coords>;
        sorted_coords = coords_to_world.keys().cloned().collect();
        sorted_coords.sort();
        assert_eq!(sorted_coords.len(), 825);
        let mut coords_to_index: HashMap<Coords, usize> = HashMap::new();
        for (ii, coords) in sorted_coords.iter_mut().enumerate() {
            coords_to_index.insert(*coords, ii);
            let world_opt = coords_to_world.get_mut(coords);
            if let Some(world) = world_opt {
                world.index = Some(ii);
            } else {
                panic!("World not found at coords");
            }
        }
        let (dist2, _) = populate_navigable_distances(&sorted_coords, &coords_to_world, 2);
        let (dist3, _) = populate_navigable_distances(&sorted_coords, &coords_to_world, 3);

        let aramis = spin
            .hex_to_world("3110".to_string(), &coords_to_world)
            .unwrap();
        let ldd = spin
            .hex_to_world("3010".to_string(), &coords_to_world)
            .unwrap();
        let natoko = spin
            .hex_to_world("3209".to_string(), &coords_to_world)
            .unwrap();
        let vinorian = spin
            .hex_to_world("3111".to_string(), &coords_to_world)
            .unwrap();
        let margesi = spin
            .hex_to_world("3212".to_string(), &coords_to_world)
            .unwrap();
        let corfu = spin
            .hex_to_world("2602".to_string(), &coords_to_world)
            .unwrap();
        let andor = spin
            .hex_to_world("0236".to_string(), &coords_to_world)
            .unwrap();
        let candory = spin
            .hex_to_world("0336".to_string(), &coords_to_world)
            .unwrap();
        let reno = spin
            .hex_to_world("0102".to_string(), &coords_to_world)
            .unwrap();
        let regina = spin
            .hex_to_world("1910".to_string(), &coords_to_world)
            .unwrap();
        let mongo = spin
            .hex_to_world("1204".to_string(), &coords_to_world)
            .unwrap();
        let collace = spin
            .hex_to_world("1237".to_string(), &coords_to_world)
            .unwrap();
        let pavanne = spin
            .hex_to_world("2905".to_string(), &coords_to_world)
            .unwrap();
        let raweh = spin
            .hex_to_world("0139".to_string(), &coords_to_world)
            .unwrap();
        let javan = dene
            .hex_to_world("2131".to_string(), &coords_to_world)
            .unwrap();
        let salaam = dene
            .hex_to_world("3213".to_string(), &coords_to_world)
            .unwrap();

        assert_eq!(aramis.navigable_distance(aramis, &dist2), 0);
        assert_eq!(aramis.navigable_distance(aramis, &dist3), 0);
        assert_eq!(aramis.navigable_distance(ldd, &dist2), 1);
        assert_eq!(aramis.navigable_distance(ldd, &dist3), 1);
        assert_eq!(aramis.navigable_distance(vinorian, &dist2), 1);
        assert_eq!(aramis.navigable_distance(vinorian, &dist3), 1);
        assert_eq!(aramis.navigable_distance(corfu, &dist2), 16);
        assert_eq!(aramis.navigable_distance(corfu, &dist3), 13);
        assert_eq!(aramis.navigable_distance(andor, &dist2), INFINITY);
        assert_eq!(aramis.navigable_distance(andor, &dist3), 45);
        assert_eq!(aramis.navigable_distance(margesi, &dist2), 3);
        assert_eq!(aramis.navigable_distance(pavanne, &dist2), 6);
        assert_eq!(aramis.navigable_distance(regina, &dist2), 12);
        assert_eq!(aramis.navigable_distance(mongo, &dist2), 22);
        assert_eq!(aramis.navigable_distance(collace, &dist2), 37);
        assert_eq!(reno.navigable_distance(javan, &dist2), 61);
        assert_eq!(andor.navigable_distance(candory, &dist2), INFINITY);
        assert_eq!(candory.navigable_distance(andor, &dist2), INFINITY);
        assert_eq!(ldd.navigable_distance(natoko, &dist2), 2);
        assert_eq!(collace.navigable_distance(salaam, &dist2), 59);
        assert_eq!(raweh.navigable_distance(salaam, &dist2), 70);

        Ok(())
    }

    #[rstest]
    fn test_navigable_path(data_dir: &PathBuf, download: &Result<Vec<String>>) -> Result<()> {
        if let Ok(_sector_names) = download {};
        let mut coords_to_world: HashMap<Coords, World> = HashMap::new();
        let mut location_to_sector: HashMap<(i64, i64), Sector> = HashMap::new();
        let spin = Sector::new(
            &data_dir,
            "Spinward Marches".to_string(),
            &mut coords_to_world,
        );
        let dene = Sector::new(&data_dir, "Deneb".to_string(), &mut coords_to_world);
        location_to_sector.insert(spin.location, spin.clone());
        location_to_sector.insert(dene.location, dene.clone());
        for sector in location_to_sector.values() {
            sector
                .parse_xml_routes(&data_dir, &location_to_sector, &mut coords_to_world)
                .unwrap();
        }
        // Make a temporary clone to avoid having mutable and immutable refs.
        let coords_to_world2 = coords_to_world.clone();
        for world in coords_to_world.values_mut() {
            world.populate_neighbors(&coords_to_world2);
        }
        let mut sorted_coords: Vec<Coords>;
        sorted_coords = coords_to_world.keys().cloned().collect();
        assert_eq!(sorted_coords.len(), 825);
        sorted_coords.sort();
        let mut coords_to_index: HashMap<Coords, usize> = HashMap::new();
        for (ii, coords) in sorted_coords.iter_mut().enumerate() {
            coords_to_index.insert(*coords, ii);
            let world_opt = coords_to_world.get_mut(coords);
            if let Some(world) = world_opt {
                world.index = Some(ii);
            } else {
                panic!("World not found at coords");
            }
        }
        let (dist2, pred2) = populate_navigable_distances(&sorted_coords, &coords_to_world, 2);
        let (dist3, pred3) = populate_navigable_distances(&sorted_coords, &coords_to_world, 3);

        let aramis = spin
            .hex_to_world("3110".to_string(), &coords_to_world)
            .unwrap();
        let ldd = spin
            .hex_to_world("3010".to_string(), &coords_to_world)
            .unwrap();
        let vinorian = spin
            .hex_to_world("3111".to_string(), &coords_to_world)
            .unwrap();
        let corfu = spin
            .hex_to_world("2602".to_string(), &coords_to_world)
            .unwrap();
        let andor = spin
            .hex_to_world("0236".to_string(), &coords_to_world)
            .unwrap();
        let candory = spin
            .hex_to_world("0336".to_string(), &coords_to_world)
            .unwrap();
        let reno = spin
            .hex_to_world("0102".to_string(), &coords_to_world)
            .unwrap();
        let mongo = spin
            .hex_to_world("1204".to_string(), &coords_to_world)
            .unwrap();
        let collace = spin
            .hex_to_world("1237".to_string(), &coords_to_world)
            .unwrap();
        let javan = dene
            .hex_to_world("2131".to_string(), &coords_to_world)
            .unwrap();
        let pysadi = spin
            .hex_to_world("3008".to_string(), &coords_to_world)
            .unwrap();
        let lewis = spin
            .hex_to_world("3107".to_string(), &coords_to_world)
            .unwrap();
        let yebab = spin
            .hex_to_world("3002".to_string(), &coords_to_world)
            .unwrap();
        let lablon = spin
            .hex_to_world("2701".to_string(), &coords_to_world)
            .unwrap();
        let violante = spin
            .hex_to_world("2708".to_string(), &coords_to_world)
            .unwrap();
        let focaline = spin
            .hex_to_world("2607".to_string(), &coords_to_world)
            .unwrap();
        let moughas = spin
            .hex_to_world("2406".to_string(), &coords_to_world)
            .unwrap();
        let enope = spin
            .hex_to_world("2205".to_string(), &coords_to_world)
            .unwrap();
        let becks_world = spin
            .hex_to_world("2204".to_string(), &coords_to_world)
            .unwrap();
        let yorbund = spin
            .hex_to_world("2303".to_string(), &coords_to_world)
            .unwrap();
        let heya = spin
            .hex_to_world("2402".to_string(), &coords_to_world)
            .unwrap();
        let zila = spin
            .hex_to_world("2908".to_string(), &coords_to_world)
            .unwrap();
        let zykoca = spin
            .hex_to_world("3004".to_string(), &coords_to_world)
            .unwrap();
        let feri = spin
            .hex_to_world("2005".to_string(), &coords_to_world)
            .unwrap();
        let uakye = spin
            .hex_to_world("1805".to_string(), &coords_to_world)
            .unwrap();
        let efate = spin
            .hex_to_world("1705".to_string(), &coords_to_world)
            .unwrap();
        let lysen = spin
            .hex_to_world("1307".to_string(), &coords_to_world)
            .unwrap();
        let nakege = spin
            .hex_to_world("1305".to_string(), &coords_to_world)
            .unwrap();

        let nutema = spin
            .hex_to_world("3112".to_string(), &coords_to_world)
            .unwrap();
        let celepina = spin
            .hex_to_world("2913".to_string(), &coords_to_world)
            .unwrap();
        let jae_tellona = spin
            .hex_to_world("2814".to_string(), &coords_to_world)
            .unwrap();
        let rhylanor = spin
            .hex_to_world("2716".to_string(), &coords_to_world)
            .unwrap();
        let equus = spin
            .hex_to_world("2417".to_string(), &coords_to_world)
            .unwrap();
        let ivendo = spin
            .hex_to_world("2319".to_string(), &coords_to_world)
            .unwrap();
        let quiru = spin
            .hex_to_world("2321".to_string(), &coords_to_world)
            .unwrap();
        let resten = spin
            .hex_to_world("2323".to_string(), &coords_to_world)
            .unwrap();
        let lunion = spin
            .hex_to_world("2124".to_string(), &coords_to_world)
            .unwrap();
        let derchon = spin
            .hex_to_world("2024".to_string(), &coords_to_world)
            .unwrap();
        let zaibon = spin
            .hex_to_world("1825".to_string(), &coords_to_world)
            .unwrap();
        let iron = spin
            .hex_to_world("1626".to_string(), &coords_to_world)
            .unwrap();
        let mithril = spin
            .hex_to_world("1628".to_string(), &coords_to_world)
            .unwrap();
        let steel = spin
            .hex_to_world("1529".to_string(), &coords_to_world)
            .unwrap();
        let dawnworld = spin
            .hex_to_world("1531".to_string(), &coords_to_world)
            .unwrap();
        let forine = spin
            .hex_to_world("1533".to_string(), &coords_to_world)
            .unwrap();
        let tarkine = spin
            .hex_to_world("1434".to_string(), &coords_to_world)
            .unwrap();
        let talos = spin
            .hex_to_world("1436".to_string(), &coords_to_world)
            .unwrap();

        let path_opt =
            aramis.navigable_path(aramis, &sorted_coords, &coords_to_index, &dist2, &pred2);
        if let Some(path) = path_opt {
            assert_eq!(path.len(), 1);
            assert_eq!(path[0], aramis.get_coords());
        } else {
            panic!("No navigable path");
        }

        let path_opt =
            aramis.navigable_path(aramis, &sorted_coords, &coords_to_index, &dist3, &pred3);
        if let Some(path) = path_opt {
            assert_eq!(path.len(), 1);
            assert_eq!(path[0], aramis.get_coords());
        } else {
            panic!("No navigable path");
        }

        let path_opt = aramis.navigable_path(ldd, &sorted_coords, &coords_to_index, &dist2, &pred2);
        if let Some(path) = path_opt {
            assert_eq!(path.len(), 2);
            assert_eq!(path[0], aramis.get_coords());
            assert_eq!(path[1], ldd.get_coords());
        } else {
            panic!("No navigable path");
        }

        let path_opt = aramis.navigable_path(ldd, &sorted_coords, &coords_to_index, &dist3, &pred3);
        if let Some(path) = path_opt {
            assert_eq!(path.len(), 2);
            assert_eq!(path[0], aramis.get_coords());
            assert_eq!(path[1], ldd.get_coords());
        } else {
            panic!("No navigable path");
        }

        let path_opt =
            aramis.navigable_path(vinorian, &sorted_coords, &coords_to_index, &dist2, &pred2);
        if let Some(path) = path_opt {
            assert_eq!(path.len(), 2);
            assert_eq!(path[0], aramis.get_coords());
            assert_eq!(path[1], vinorian.get_coords());
        } else {
            panic!("No navigable path");
        }

        let path_opt =
            aramis.navigable_path(vinorian, &sorted_coords, &coords_to_index, &dist3, &pred3);
        if let Some(path) = path_opt {
            assert_eq!(path.len(), 2);
            assert_eq!(path[0], aramis.get_coords());
            assert_eq!(path[1], vinorian.get_coords());
        } else {
            panic!("No navigable path");
        }

        let path_opt =
            aramis.navigable_path(corfu, &sorted_coords, &coords_to_index, &dist2, &pred2);
        if let Some(path) = path_opt {
            assert_eq!(path.len(), 11);
            for coords in &path {
                println!("{}", coords_to_world.get(&coords).unwrap().name);
            }
            assert_eq!(
                path,
                vec![
                    aramis.get_coords(),
                    pysadi.get_coords(),
                    zila.get_coords(),
                    violante.get_coords(),
                    focaline.get_coords(),
                    moughas.get_coords(),
                    enope.get_coords(),
                    becks_world.get_coords(),
                    yorbund.get_coords(),
                    heya.get_coords(),
                    corfu.get_coords(),
                ]
            );
        } else {
            panic!("No navigable path");
        }

        let path_opt =
            aramis.navigable_path(corfu, &sorted_coords, &coords_to_index, &dist3, &pred3);
        if let Some(path) = path_opt {
            for coords in &path {
                println!("{}", coords_to_world.get(&coords).unwrap().name);
            }
            assert_eq!(path.len(), 6);
            assert_eq!(
                path,
                vec![
                    aramis.get_coords(),
                    lewis.get_coords(),
                    zykoca.get_coords(),
                    yebab.get_coords(),
                    lablon.get_coords(),
                    corfu.get_coords(),
                ]
            );
        } else {
            panic!("No navigable path");
        }

        let path_opt =
            aramis.navigable_path(mongo, &sorted_coords, &coords_to_index, &dist2, &pred2);
        if let Some(path) = path_opt {
            for coords in &path {
                println!("{}", coords_to_world.get(&coords).unwrap().name);
            }
            assert_eq!(path.len(), 13);
            assert_eq!(
                path,
                vec![
                    aramis.get_coords(),
                    pysadi.get_coords(),
                    zila.get_coords(),
                    violante.get_coords(),
                    focaline.get_coords(),
                    moughas.get_coords(),
                    enope.get_coords(),
                    feri.get_coords(),
                    uakye.get_coords(),
                    efate.get_coords(),
                    lysen.get_coords(),
                    nakege.get_coords(),
                    mongo.get_coords(),
                ]
            );
        } else {
            panic!("No navigable path");
        }

        let path_opt =
            aramis.navigable_path(collace, &sorted_coords, &coords_to_index, &dist2, &pred2);
        if let Some(path) = path_opt {
            for coords in &path {
                println!("{}", coords_to_world.get(&coords).unwrap().name);
            }
            assert_eq!(path.len(), 20);
            assert_eq!(
                path,
                vec![
                    aramis.get_coords(),
                    nutema.get_coords(),
                    celepina.get_coords(),
                    jae_tellona.get_coords(),
                    rhylanor.get_coords(),
                    equus.get_coords(),
                    ivendo.get_coords(),
                    quiru.get_coords(),
                    resten.get_coords(),
                    lunion.get_coords(),
                    derchon.get_coords(),
                    zaibon.get_coords(),
                    iron.get_coords(),
                    mithril.get_coords(),
                    steel.get_coords(),
                    dawnworld.get_coords(),
                    forine.get_coords(),
                    tarkine.get_coords(),
                    talos.get_coords(),
                    collace.get_coords(),
                ]
            );
        } else {
            panic!("No navigable path");
        }

        let path_opt = reno.navigable_path(javan, &sorted_coords, &coords_to_index, &dist2, &pred2);
        if let Some(path) = path_opt {
            for coords in &path {
                println!("{}", coords_to_world.get(&coords).unwrap().name);
            }
            assert_eq!(path.len(), 33);
        } else {
            panic!("No navigable path");
        }

        let path_opt =
            andor.navigable_path(candory, &sorted_coords, &coords_to_index, &dist2, &pred2);
        assert_eq!(path_opt, None);

        let path_opt =
            candory.navigable_path(andor, &sorted_coords, &coords_to_index, &dist2, &pred2);
        assert_eq!(path_opt, None);

        let path_opt =
            aramis.navigable_path(andor, &sorted_coords, &coords_to_index, &dist2, &pred2);
        assert_eq!(path_opt, None);

        let path_opt =
            aramis.navigable_path(andor, &sorted_coords, &coords_to_index, &dist3, &pred3);
        if let Some(path) = path_opt {
            for coords in &path {
                println!("{}", coords_to_world.get(&coords).unwrap().name);
            }
            assert_eq!(path.len(), 17);
        } else {
            panic!("No navigable path");
        }

        Ok(())
    }
}
