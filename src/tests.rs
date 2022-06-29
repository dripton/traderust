use anyhow::Result;
use ndarray::Array2;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::fs::{create_dir_all, remove_dir_all};
use std::path::PathBuf;

use crate::apsp::{Algorithm, INFINITY};
use crate::pdf::generate_pdfs;
use crate::{
    distance_modifier_table, download_sector_data, find_max_allowed_jump,
    parse_header_and_separator, populate_navigable_distances, populate_trade_routes,
    same_allegiance, Route, MAX_DISTANCE_PENALTY, MIN_BTN, MIN_ROUTE_BTN,
};
use crate::{Coords, Sector, World};
use Route::{Feeder, Intermediate, Main, Major, Minor};

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::*;
    use std::ffi::OsString;
    use std::fs::read_dir;
    use std::io;

    // Reuse a test directory and downloaded files to avoid overloading travellermap.com
    const TEST_DATA_DIR: &'static str = "/var/tmp/traderust_tests";

    const TEST_OUTPUT_DIR: &'static str = "/var/tmp/traderust_tests_output";

    const ALG: Algorithm = Algorithm::Dijkstra;

    macro_rules! htw {
        ($sector:expr, $hex:expr, $ctw:expr) => {
            $sector.hex_to_world(format!("{:04}", $hex), &$ctw).unwrap()
        };
    }

    macro_rules! set {
        ( $( $val:expr ),* ) => {
            {
                let mut my_set = HashSet::new();
                $(
                    my_set.insert($val);
                )*
                my_set
            }
        };
    }

    #[fixture]
    #[once]
    fn data_dir() -> PathBuf {
        let data_dir = PathBuf::from(TEST_DATA_DIR);
        create_dir_all(&data_dir).unwrap();
        data_dir
    }

    #[fixture]
    #[once]
    fn output_dir() -> PathBuf {
        let output_dir = PathBuf::from(TEST_OUTPUT_DIR);
        remove_dir_all(&output_dir).unwrap();
        create_dir_all(&output_dir).unwrap();
        output_dir
    }

    #[fixture]
    #[once]
    fn download(data_dir: &PathBuf) -> Result<Vec<String>> {
        let sector_names = vec![
            "Deneb".to_string(),
            "Gvurrdon".to_string(),
            "Spinward Marches".to_string(),
            "Core".to_string(),
            "Reft".to_string(),
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
    fn test_coords_ord() {
        let mut sorted_coords = Vec::new();
        let mut x = -101.0;
        let mut y = -101.0;
        while x <= 101.0 {
            while y <= 101.0 {
                let coords = Coords::new(x, y);
                sorted_coords.push(coords);
                y += 0.5;
            }
            x += 1.0;
        }
        sorted_coords.sort();
        for (ii, coords1) in sorted_coords.iter().enumerate() {
            for jj in ii + 1..sorted_coords.len() {
                let coords2 = sorted_coords[jj];
                assert!(
                    coords2.x > coords1.x || (coords2.x == coords1.x && coords2.y2 > coords1.y2)
                );
            }
        }
    }

    #[rstest]
    fn test_coords_ord2() {
        let coords1 = Coords::new(0.0, 0.0);
        let coords2 = Coords::new(1.0, 1.0);
        assert_eq!(coords1.cmp(&coords1), Ordering::Equal);
        assert_eq!(coords1.cmp(&coords2), Ordering::Less);
        assert_eq!(coords2.cmp(&coords1), Ordering::Greater);
    }

    #[rstest]
    fn test_coords_partial_ord() {
        let coords1 = Coords::new(0.0, 0.0);
        let coords2 = Coords::new(1.0, 1.0);
        assert_eq!(coords1.partial_cmp(&coords1), Some(Ordering::Equal));
        assert_eq!(coords1.partial_cmp(&coords2), Some(Ordering::Less));
        assert_eq!(coords2.partial_cmp(&coords1), Some(Ordering::Greater));
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

        assert_eq!(sector.name, "Spinward Marches");
        assert_eq!(sector.names, vec!["Spinward Marches", "Tloql"]);
        assert_eq!(sector.abbreviation, "Spin");
        assert_eq!(sector.location, (-4, -1));
        assert_eq!(sector.subsector_letter_to_name.len(), 16);
        assert_eq!(
            *sector.subsector_letter_to_name.get(&'A').unwrap(),
            "Cronor".to_string()
        );
        assert_eq!(
            *sector.subsector_letter_to_name.get(&'P').unwrap(),
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

        assert_eq!(sector.name, "Deneb");
        assert_eq!(sector.names, vec!["Deneb", "Nieklsdia"]);
        assert_eq!(sector.abbreviation, "Dene");
        assert_eq!(sector.location, (-3, -1));
        assert_eq!(sector.subsector_letter_to_name.len(), 16);
        assert_eq!(
            *sector.subsector_letter_to_name.get(&'A').unwrap(),
            "Pretoria".to_string()
        );
        assert_eq!(
            *sector.subsector_letter_to_name.get(&'P').unwrap(),
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

        assert_eq!(sector.name, "Gvurrdon");
        assert_eq!(sector.names, vec!["Gvurrdon", r"Briakqra'"]);
        assert_eq!(sector.abbreviation, "Gvur");
        assert_eq!(sector.location, (-4, -2));
        assert_eq!(sector.subsector_letter_to_name.len(), 16);
        assert_eq!(
            *sector.subsector_letter_to_name.get(&'A').unwrap(),
            "Ongvos".to_string()
        );
        assert_eq!(
            *sector.subsector_letter_to_name.get(&'P').unwrap(),
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
    fn test_sector_core(data_dir: &PathBuf, download: &Result<Vec<String>>) -> Result<()> {
        if let Ok(_sector_names) = download {};
        let sector_name = "Core".to_string();
        let mut coords_to_world: HashMap<Coords, World> = HashMap::new();
        let sector = Sector::new(&data_dir, sector_name, &mut coords_to_world);

        assert_eq!(sector.name, "Core");
        assert_eq!(sector.names, vec!["Core", "Ukan"]);
        assert_eq!(sector.abbreviation, "Core");
        assert_eq!(sector.location, (0, 0));
        assert_eq!(sector.subsector_letter_to_name.len(), 16);
        assert_eq!(
            *sector.subsector_letter_to_name.get(&'A').unwrap(),
            "Apge".to_string()
        );
        assert_eq!(
            *sector.subsector_letter_to_name.get(&'P').unwrap(),
            "Saregon".to_string()
        );
        assert_eq!(sector.allegiance_code_to_name.len(), 2);
        assert_eq!(
            *sector.allegiance_code_to_name.get("ImSy").unwrap(),
            "Third Imperium, Sylean Worlds".to_string()
        );
        assert_eq!(sector.hex_to_coords.len(), 546);
        let shana_ma_coords = sector.hex_to_coords.get("0104").unwrap();
        let shana_ma = coords_to_world.get(shana_ma_coords).unwrap();
        assert_eq!(shana_ma.name, "Shana Ma");
        let lishide_coords = sector.hex_to_coords.get("3238").unwrap();
        let lishide = coords_to_world.get(lishide_coords).unwrap();
        assert_eq!(lishide.name, "Lishide");

        Ok(())
    }

    #[rstest]
    fn test_sector_reft(data_dir: &PathBuf, download: &Result<Vec<String>>) -> Result<()> {
        if let Ok(_sector_names) = download {};
        let sector_name = "Reft".to_string();
        let mut coords_to_world: HashMap<Coords, World> = HashMap::new();
        let sector = Sector::new(&data_dir, sector_name, &mut coords_to_world);

        assert_eq!(sector.name, "Reft");
        assert_eq!(
            sector.names,
            vec!["Reft", r"Reft Sector", "Bransakral", "Sushinar"]
        );
        assert_eq!(sector.abbreviation, "Reft");
        assert_eq!(sector.location, (-3, 0));
        assert_eq!(sector.subsector_letter_to_name.len(), 16);
        assert_eq!(
            *sector.subsector_letter_to_name.get(&'A').unwrap(),
            "Vestus".to_string()
        );
        assert_eq!(
            *sector.subsector_letter_to_name.get(&'P').unwrap(),
            "Moibin".to_string()
        );
        assert_eq!(sector.allegiance_code_to_name.len(), 5);
        assert_eq!(
            *sector.allegiance_code_to_name.get("CsIm").unwrap(),
            "Client state, Third Imperium".to_string()
        );
        assert_eq!(sector.hex_to_coords.len(), 130);
        let grudovo_coords = sector.hex_to_coords.get("0111").unwrap();
        let grudovo = coords_to_world.get(grudovo_coords).unwrap();
        assert_eq!(grudovo.name, "Grudovo");
        let jeandrent_coords = sector.hex_to_coords.get("3237").unwrap();
        let jeandrent = coords_to_world.get(jeandrent_coords).unwrap();
        assert_eq!(jeandrent.name, "Jeandrent");

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
        assert_eq!(aramis.sector_name, "Spinward Marches");
        assert_eq!(aramis.hex, "3110");
        assert_eq!(aramis.uwp, "A5A0556-B");

        let tc = set!("He".to_string(), "Ni".to_string(), "Cp".to_string());
        assert_eq!(aramis.trade_classifications, tc);

        assert_eq!(aramis.importance(), 2);
        assert_eq!(aramis.economic(), "846+1");
        assert_eq!(aramis.cultural(), "474A");
        assert_eq!(aramis.nobles(), "BF");
        let bases = set!("N".to_string(), "S".to_string());
        assert_eq!(aramis.bases(), bases);
        assert_eq!(aramis.zone, 'G');
        assert_eq!(aramis.pbg, "710");
        assert_eq!(aramis.worlds(), 9);
        assert_eq!(aramis.allegiance, "ImDd");
        assert_eq!(aramis.stars(), vec!["M2 V"]);
        assert_eq!(aramis.starport(), 'A');
        assert_eq!(aramis.g_starport(), "V");
        assert_eq!(aramis.size(), '5');
        assert_eq!(aramis.atmosphere(), 'A');
        assert_eq!(aramis.hydrosphere(), '0');
        assert_eq!(aramis.population(), '5');
        assert_eq!(aramis.government(), '5');
        assert_eq!(aramis.law_level(), '6');
        assert_eq!(aramis.tech_level(), 'B');
        assert_eq!(aramis.g_tech_level(), 9);
        assert_eq!(aramis.uwtn(), 3.5);
        assert_eq!(aramis.wtn_port_modifier(), 0.5);
        assert_eq!(aramis.wtn(), 4.0);
        assert_eq!(aramis.gas_giants(), '0');
        assert!(aramis.can_refuel(false));
        assert_eq!(aramis.desc(), "Aramis (Spinward Marches 3110)");

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
        assert_eq!(regina.sector_name, "Spinward Marches");
        assert_eq!(regina.hex, "1910");
        assert_eq!(regina.uwp, "A788899-C");

        let tc = set!(
            "Ri".to_string(),
            "Pa".to_string(),
            "Ph".to_string(),
            "An".to_string(),
            "Cp".to_string(),
            "(Amindii)2".to_string(),
            "Varg0".to_string(),
            "Asla0".to_string(),
            "Sa".to_string()
        );
        assert_eq!(regina.trade_classifications, tc);

        assert_eq!(regina.importance(), 4);
        assert_eq!(regina.economic(), "D7E+5");
        assert_eq!(regina.cultural(), "9C6D");
        assert_eq!(regina.nobles(), "BcCeF");
        let bases = set!("N".to_string(), "S".to_string());
        assert_eq!(regina.bases(), bases);
        assert_eq!(regina.zone, 'G');
        assert_eq!(regina.pbg, "703");
        assert_eq!(regina.worlds(), 8);
        assert_eq!(regina.allegiance, "ImDd");
        assert_eq!(regina.stars(), vec!["F7 V", "BD", "M3 V"]);
        assert_eq!(regina.starport(), 'A');
        assert_eq!(regina.g_starport(), "V");
        assert_eq!(regina.size(), '7');
        assert_eq!(regina.atmosphere(), '8');
        assert_eq!(regina.hydrosphere(), '8');
        assert_eq!(regina.population(), '8');
        assert_eq!(regina.government(), '9');
        assert_eq!(regina.law_level(), '9');
        assert_eq!(regina.tech_level(), 'C');
        assert_eq!(regina.g_tech_level(), 10);
        assert_eq!(regina.uwtn(), 5.0);
        assert_eq!(regina.wtn_port_modifier(), 0.0);
        assert_eq!(regina.wtn(), 5.0);
        assert_eq!(regina.gas_giants(), '3');
        assert!(regina.can_refuel(false));
        assert_eq!(regina.desc(), "Regina (Spinward Marches 1910)");

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
        assert_eq!(bronze.sector_name, "Spinward Marches");
        assert_eq!(bronze.hex, "1627");
        assert_eq!(bronze.uwp, "E201000-0");

        let tc = set!(
            "Ba".to_string(),
            "Ic".to_string(),
            "Re".to_string(),
            "Va".to_string()
        );
        assert_eq!(bronze.trade_classifications, tc);

        assert_eq!(bronze.importance(), -3);
        assert_eq!(bronze.economic(), "200-5");
        assert_eq!(bronze.cultural(), "0000");
        assert_eq!(bronze.nobles(), "");
        let bases = HashSet::new();
        assert_eq!(bronze.bases(), bases);
        assert_eq!(bronze.zone, 'G');
        assert_eq!(bronze.pbg, "010");
        assert_eq!(bronze.worlds(), 5);
        assert_eq!(bronze.allegiance, "SwCf");
        assert_eq!(bronze.stars(), vec!["M3 V"]);
        assert_eq!(bronze.starport(), 'E');
        assert_eq!(bronze.g_starport(), "I");
        assert_eq!(bronze.size(), '2');
        assert_eq!(bronze.atmosphere(), '0');
        assert_eq!(bronze.hydrosphere(), '1');
        assert_eq!(bronze.population(), '0');
        assert_eq!(bronze.government(), '0');
        assert_eq!(bronze.law_level(), '0');
        assert_eq!(bronze.tech_level(), '0');
        assert_eq!(bronze.g_tech_level(), 2);
        assert_eq!(bronze.uwtn(), -0.5);
        assert_eq!(bronze.wtn_port_modifier(), 0.5);
        assert_eq!(bronze.wtn(), 0.0);
        assert_eq!(bronze.gas_giants(), '0');
        assert!(bronze.can_refuel(false));
        assert_eq!(bronze.desc(), "Bronze (Spinward Marches 1627)");

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
        assert_eq!(callia.sector_name, "Spinward Marches");
        assert_eq!(callia.hex, "1836");
        assert_eq!(callia.uwp, "E550852-6");

        let tc = set!("De".to_string(), "Po".to_string(), "Ph".to_string());
        assert_eq!(callia.trade_classifications, tc);

        assert_eq!(callia.importance(), -2);
        assert_eq!(callia.economic(), "A75-5");
        assert_eq!(callia.cultural(), "4612");
        assert_eq!(callia.nobles(), "Be");
        let bases = HashSet::new();
        assert_eq!(callia.bases(), bases);
        assert_eq!(callia.zone, 'G');
        assert_eq!(callia.pbg, "810");
        assert_eq!(callia.worlds(), 11);
        assert_eq!(callia.allegiance, "ImDd");
        assert_eq!(callia.stars(), vec!["M3 V"]);
        assert_eq!(callia.starport(), 'E');
        assert_eq!(callia.g_starport(), "I");
        assert_eq!(callia.size(), '5');
        assert_eq!(callia.atmosphere(), '5');
        assert_eq!(callia.hydrosphere(), '0');
        assert_eq!(callia.population(), '8');
        assert_eq!(callia.government(), '5');
        assert_eq!(callia.law_level(), '2');
        assert_eq!(callia.tech_level(), '6');
        assert_eq!(callia.g_tech_level(), 6);
        assert_eq!(callia.uwtn(), 4.5);
        assert_eq!(callia.wtn_port_modifier(), -1.0);
        assert_eq!(callia.wtn(), 3.5);
        assert_eq!(callia.gas_giants(), '0');
        assert!(!callia.can_refuel(false));
        assert_eq!(callia.desc(), "Callia (Spinward Marches 1836)");

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
        assert_eq!(candory.sector_name, "Spinward Marches");
        assert_eq!(candory.hex, "0336");
        assert_eq!(candory.uwp, "C593634-8");

        let tc = set!(
            "Ni".to_string(),
            "An".to_string(),
            "Fo".to_string(),
            "DroyW".to_string()
        );
        assert_eq!(candory.trade_classifications, tc);

        assert_eq!(candory.importance(), -2);
        assert_eq!(candory.economic(), "A52-4");
        assert_eq!(candory.cultural(), "4436");
        assert_eq!(candory.nobles(), "");
        let bases = HashSet::new();
        assert_eq!(candory.bases, bases);
        assert_eq!(candory.zone, 'R');
        assert_eq!(candory.pbg, "920");
        assert_eq!(candory.worlds(), 5);
        assert_eq!(candory.allegiance, "ImDd");
        assert_eq!(candory.stars(), vec!["F6 V", "M3 V"]);
        assert_eq!(candory.starport(), 'C');
        assert_eq!(candory.g_starport(), "III");
        assert_eq!(candory.size(), '5');
        assert_eq!(candory.atmosphere(), '9');
        assert_eq!(candory.hydrosphere(), '3');
        assert_eq!(candory.population(), '6');
        assert_eq!(candory.government(), '3');
        assert_eq!(candory.law_level(), '4');
        assert_eq!(candory.tech_level(), '8');
        assert_eq!(candory.g_tech_level(), 8);
        assert_eq!(candory.uwtn(), 3.5);
        assert_eq!(candory.wtn_port_modifier(), 0.0);
        assert_eq!(candory.wtn(), 3.5);
        assert_eq!(candory.gas_giants(), '0');
        assert!(!candory.can_refuel(false));
        assert!(!candory.can_refuel(true));
        assert_eq!(candory.desc(), "Candory (Spinward Marches 0336)");

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
        assert_eq!(khiinra_ash.sector_name, "Core");
        assert_eq!(khiinra_ash.hex, "2916");
        assert_eq!(khiinra_ash.uwp, "BAE6362-8");
        // No test for trade classifications to avoid UTF-8 in the code
        assert_eq!(khiinra_ash.importance(), -1);
        assert_eq!(khiinra_ash.economic(), "920-5");
        assert_eq!(khiinra_ash.cultural(), "1214");
        assert_eq!(khiinra_ash.nobles(), "B");
        let bases = HashSet::new();
        assert_eq!(khiinra_ash.bases(), bases);
        assert_eq!(khiinra_ash.zone, 'G');
        assert_eq!(khiinra_ash.pbg, "704");
        assert_eq!(khiinra_ash.worlds(), 7);
        assert_eq!(khiinra_ash.allegiance, "ImSy");
        assert_eq!(khiinra_ash.stars(), vec!["M1 V", "M2 V"]);
        assert_eq!(khiinra_ash.starport(), 'B');
        assert_eq!(khiinra_ash.g_starport(), "IV");
        assert_eq!(khiinra_ash.size(), 'A');
        assert_eq!(khiinra_ash.atmosphere(), 'E');
        assert_eq!(khiinra_ash.hydrosphere(), '6');
        assert_eq!(khiinra_ash.population(), '3');
        assert_eq!(khiinra_ash.government(), '6');
        assert_eq!(khiinra_ash.law_level(), '2');
        assert_eq!(khiinra_ash.tech_level(), '8');
        assert_eq!(khiinra_ash.g_tech_level(), 8);
        assert_eq!(khiinra_ash.uwtn(), 2.0);
        assert_eq!(khiinra_ash.wtn_port_modifier(), 0.5);
        assert_eq!(khiinra_ash.wtn(), 2.5);
        assert_eq!(khiinra_ash.gas_giants(), '4');
        assert!(khiinra_ash.can_refuel(false));
        assert_eq!(khiinra_ash.desc(), "Khiinra Ash (Core 2916)");

        Ok(())
    }

    #[rstest]
    fn test_get_coords(data_dir: &PathBuf, download: &Result<Vec<String>>) -> Result<()> {
        if let Ok(_sector_names) = download {};
        let mut coords_to_world: HashMap<Coords, World> = HashMap::new();
        let spin = Sector::new(
            &data_dir,
            "Spinward Marches".to_string(),
            &mut coords_to_world,
        );
        let dene = Sector::new(&data_dir, "Deneb".to_string(), &mut coords_to_world);

        let aramis = htw!(spin, 3110, coords_to_world);
        let ldd = htw!(spin, 3010, coords_to_world);
        let natoko = htw!(spin, 3209, coords_to_world);
        let reacher = htw!(spin, 3210, coords_to_world);
        let vinorian = htw!(spin, 3111, coords_to_world);
        let nutema = htw!(spin, 3112, coords_to_world);
        let margesi = htw!(spin, 3212, coords_to_world);
        let saarinen = htw!(dene, 0113, coords_to_world);
        let regina = htw!(spin, 1910, coords_to_world);

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

        let aramis = htw!(spin, 3110, coords_to_world);
        let ldd = htw!(spin, 3010, coords_to_world);
        let natoko = htw!(spin, 3209, coords_to_world);
        let reacher = htw!(spin, 3210, coords_to_world);
        let vinorian = htw!(spin, 3111, coords_to_world);
        let nutema = htw!(spin, 3112, coords_to_world);
        let margesi = htw!(spin, 3212, coords_to_world);
        let patinir = htw!(spin, 3207, coords_to_world);
        let saarinen = htw!(dene, 0113, coords_to_world);
        let regina = htw!(spin, 1910, coords_to_world);
        let corfu = htw!(spin, 2602, coords_to_world);
        let lablon = htw!(spin, 2701, coords_to_world);
        let junidy = htw!(spin, 3202, coords_to_world);
        let marz = htw!(dene, 0201, coords_to_world);

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
    fn test_distance_modifier_table() {
        assert_eq!(distance_modifier_table(0), 0.0);
        assert_eq!(distance_modifier_table(1), 0.0);
        assert_eq!(distance_modifier_table(2), 0.5);
        assert_eq!(distance_modifier_table(3), 1.0);
        assert_eq!(distance_modifier_table(5), 1.0);
        assert_eq!(distance_modifier_table(6), 1.5);
        assert_eq!(distance_modifier_table(9), 1.5);
        assert_eq!(distance_modifier_table(10), 2.0);
        assert_eq!(distance_modifier_table(19), 2.0);
        assert_eq!(distance_modifier_table(20), 2.5);
        assert_eq!(distance_modifier_table(29), 2.5);
        assert_eq!(distance_modifier_table(30), 3.0);
        assert_eq!(distance_modifier_table(59), 3.0);
        assert_eq!(distance_modifier_table(60), 3.5);
        assert_eq!(distance_modifier_table(99), 3.5);
        assert_eq!(distance_modifier_table(100), 4.0);
        assert_eq!(distance_modifier_table(199), 4.0);
        assert_eq!(distance_modifier_table(200), 4.5);
        assert_eq!(distance_modifier_table(299), 4.5);
        assert_eq!(distance_modifier_table(300), 5.0);
        assert_eq!(distance_modifier_table(599), 5.0);
        assert_eq!(distance_modifier_table(600), 5.5);
        assert_eq!(distance_modifier_table(999), 5.5);
        assert_eq!(distance_modifier_table(1000), 6.0);
        assert_eq!(distance_modifier_table(9999), 6.0);
        assert_eq!(distance_modifier_table(INFINITY), MAX_DISTANCE_PENALTY);
    }

    #[rstest]
    fn test_same_allegiance() {
        assert!(!(same_allegiance("CsIm", "CsIm")));
        assert!(!(same_allegiance("CsZh", "CsZh")));
        assert!(!(same_allegiance("CsIm", "CsZh")));
        assert!(!(same_allegiance("NaHu", "NaHu")));
        assert!(!(same_allegiance("NaXX", "NaXX")));
        assert!(!(same_allegiance("NaHu", "NaXX")));
        assert!(!(same_allegiance("DaCf", "ImDd")));
        assert!(!(same_allegiance("ImDd", "ZhIN")));
        assert!((same_allegiance("DaCf", "DaCf")));
        assert!((same_allegiance("ImDd", "ImDd")));
        assert!((same_allegiance("SwCf", "SwCf")));
        assert!((same_allegiance("ZhIN", "ZhIN")));
    }

    #[rstest]
    fn test_distance_modifier(data_dir: &PathBuf, download: &Result<Vec<String>>) -> Result<()> {
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
            world.populate_neighbors(&coords_to_world2, 3, false);
        }
        let mut sorted_coords: Vec<Coords>;
        sorted_coords = coords_to_world.keys().cloned().collect();
        sorted_coords.sort();
        assert_eq!(sorted_coords.len(), 825);
        for (ii, coords) in sorted_coords.iter_mut().enumerate() {
            let world = coords_to_world.get_mut(coords).unwrap();
            world.index = Some(ii);
        }
        let (dist2, _) =
            populate_navigable_distances(&sorted_coords, &coords_to_world, 2, false, ALG);

        let aramis = htw!(spin, 3110, coords_to_world);
        let ldd = htw!(spin, 3010, coords_to_world);
        let natoko = htw!(spin, 3209, coords_to_world);
        let vinorian = htw!(spin, 3111, coords_to_world);
        let margesi = htw!(spin, 3212, coords_to_world);
        let corfu = htw!(spin, 2602, coords_to_world);
        let andor = htw!(spin, 0236, coords_to_world);
        let candory = htw!(spin, 0336, coords_to_world);
        let reno = htw!(spin, 0102, coords_to_world);
        let regina = htw!(spin, 1910, coords_to_world);
        let mongo = htw!(spin, 1204, coords_to_world);
        let collace = htw!(spin, 1237, coords_to_world);
        let pavanne = htw!(spin, 2905, coords_to_world);
        let raweh = htw!(spin, 0139, coords_to_world);
        let javan = htw!(dene, 2131, coords_to_world);
        let salaam = htw!(dene, 3213, coords_to_world);

        assert_eq!(aramis.distance_modifier(aramis, &dist2), 0.0);
        assert_eq!(aramis.distance_modifier(ldd, &dist2), 0.0);
        assert_eq!(aramis.distance_modifier(vinorian, &dist2), 0.0);
        assert_eq!(aramis.distance_modifier(corfu, &dist2), 2.0);
        assert_eq!(
            aramis.distance_modifier(andor, &dist2),
            MAX_DISTANCE_PENALTY
        );
        assert_eq!(aramis.distance_modifier(margesi, &dist2), 1.0);
        assert_eq!(aramis.distance_modifier(pavanne, &dist2), 1.5);
        assert_eq!(aramis.distance_modifier(regina, &dist2), 2.0);
        assert_eq!(aramis.distance_modifier(mongo, &dist2), 2.5);
        assert_eq!(aramis.distance_modifier(collace, &dist2), 3.0);
        assert_eq!(reno.distance_modifier(javan, &dist2), 3.5);
        assert_eq!(
            andor.distance_modifier(candory, &dist2),
            MAX_DISTANCE_PENALTY
        );
        assert_eq!(
            candory.distance_modifier(andor, &dist2),
            MAX_DISTANCE_PENALTY
        );
        assert_eq!(ldd.distance_modifier(natoko, &dist2), 0.5);
        assert_eq!(collace.distance_modifier(salaam, &dist2), 3.0);
        assert_eq!(raweh.distance_modifier(salaam, &dist2), 3.5);

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
        let aramis = htw!(spin, 3110, coords_to_world);
        let ldd = htw!(spin, 3010, coords_to_world);
        let natoko = htw!(spin, 3209, coords_to_world);
        let reacher = htw!(spin, 3210, coords_to_world);
        let vinorian = htw!(spin, 3111, coords_to_world);
        let nutema = htw!(spin, 3112, coords_to_world);
        let saarinen = htw!(dene, 0113, coords_to_world);
        let regina = htw!(spin, 1910, coords_to_world);
        let corfu = htw!(spin, 2602, coords_to_world);
        let lablon = htw!(spin, 2701, coords_to_world);
        let junidy = htw!(spin, 3202, coords_to_world);
        let marz = htw!(dene, 0201, coords_to_world);
        let celepina = htw!(spin, 2913, coords_to_world);
        let teh = htw!(dene, 0208, coords_to_world);
        let ash = htw!(dene, 0504, coords_to_world);
        let roup = htw!(spin, 2007, coords_to_world);
        let jenghe = htw!(spin, 1810, coords_to_world);
        let dinomn = htw!(spin, 1912, coords_to_world);
        let towers = htw!(spin, 3103, coords_to_world);

        let set = HashSet::new();
        assert_eq!(reacher.xboat_routes, set);
        assert_eq!(vinorian.xboat_routes, set);
        assert_eq!(nutema.xboat_routes, set);
        assert_eq!(saarinen.xboat_routes, set);
        assert_eq!(corfu.xboat_routes, set);
        assert_eq!(lablon.xboat_routes, set);

        let set = set!(ldd.get_coords(), natoko.get_coords());
        assert_eq!(aramis.xboat_routes, set);

        let set = set!(aramis.get_coords(), celepina.get_coords());
        assert_eq!(ldd.xboat_routes, set);

        let set = set!(aramis.get_coords(), teh.get_coords());
        assert_eq!(natoko.xboat_routes, set);

        let set = HashSet::new();
        assert_eq!(reacher.xboat_routes, set);
        assert_eq!(vinorian.xboat_routes, set);
        assert_eq!(nutema.xboat_routes, set);
        assert_eq!(saarinen.xboat_routes, set);
        assert_eq!(corfu.xboat_routes, set);
        assert_eq!(lablon.xboat_routes, set);

        let set = set!(marz.get_coords(), towers.get_coords());
        assert_eq!(junidy.xboat_routes, set);

        let set = set!(junidy.get_coords(), ash.get_coords());
        assert_eq!(marz.xboat_routes, set);

        let set = set!(roup.get_coords(), jenghe.get_coords(), dinomn.get_coords());
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
            world.populate_neighbors(&coords_to_world2, 3, false);
        }

        let aramis = htw!(spin, 3110, coords_to_world);
        let ldd = htw!(spin, 3010, coords_to_world);
        let natoko = htw!(spin, 3209, coords_to_world);
        let reacher = htw!(spin, 3210, coords_to_world);
        let vinorian = htw!(spin, 3111, coords_to_world);
        let nutema = htw!(spin, 3112, coords_to_world);
        let teh = htw!(dene, 0208, coords_to_world);
        let pysadi = htw!(spin, 3008, coords_to_world);
        let margesi = htw!(spin, 3212, coords_to_world);
        let zila = htw!(spin, 2908, coords_to_world);
        let lewis = htw!(spin, 3107, coords_to_world);
        let patinir = htw!(spin, 3207, coords_to_world);
        let henoz = htw!(spin, 2912, coords_to_world);
        let suvfoto = htw!(dene, 0211, coords_to_world);
        let kretikaa = htw!(dene, 0209, coords_to_world);
        let new_ramma = htw!(dene, 0108, coords_to_world);
        let valhalla = htw!(spin, 2811, coords_to_world);
        let saarinen = htw!(dene, 0113, coords_to_world);
        let celepina = htw!(spin, 2913, coords_to_world);
        let zivije = htw!(spin, 2812, coords_to_world);

        let set = set!(
            ldd.get_coords(),
            natoko.get_coords(),
            reacher.get_coords(),
            vinorian.get_coords()
        );
        assert_eq!(aramis.neighbors[1], set);

        let set = set!(nutema.get_coords(), pysadi.get_coords());
        assert_eq!(aramis.neighbors[2], set);

        let set = set!(
            margesi.get_coords(),
            teh.get_coords(),
            zila.get_coords(),
            lewis.get_coords(),
            patinir.get_coords(),
            henoz.get_coords(),
            suvfoto.get_coords(),
            kretikaa.get_coords(),
            new_ramma.get_coords(),
            valhalla.get_coords()
        );
        assert_eq!(aramis.neighbors[3], set);

        let set = set!(
            aramis.get_coords(),
            ldd.get_coords(),
            reacher.get_coords(),
            nutema.get_coords()
        );
        assert_eq!(vinorian.neighbors[1], set);

        let set = set!(
            natoko.get_coords(),
            margesi.get_coords(),
            henoz.get_coords()
        );
        assert_eq!(vinorian.neighbors[2], set);

        let set = set!(
            kretikaa.get_coords(),
            suvfoto.get_coords(),
            saarinen.get_coords(),
            celepina.get_coords(),
            zivije.get_coords(),
            valhalla.get_coords(),
            pysadi.get_coords()
        );
        assert_eq!(vinorian.neighbors[3], set);

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
            world.populate_neighbors(&coords_to_world2, 3, false);
        }
        let mut sorted_coords: Vec<Coords>;
        sorted_coords = coords_to_world.keys().cloned().collect();
        sorted_coords.sort();
        assert_eq!(sorted_coords.len(), 825);
        for (ii, coords) in sorted_coords.iter_mut().enumerate() {
            let world = coords_to_world.get_mut(coords).unwrap();
            world.index = Some(ii);
        }
        let (dist2, _) =
            populate_navigable_distances(&sorted_coords, &coords_to_world, 2, false, ALG);
        let (dist3, _) =
            populate_navigable_distances(&sorted_coords, &coords_to_world, 3, false, ALG);

        let aramis = htw!(spin, 3110, coords_to_world);
        let ldd = htw!(spin, 3010, coords_to_world);
        let natoko = htw!(spin, 3209, coords_to_world);
        let vinorian = htw!(spin, 3111, coords_to_world);
        let margesi = htw!(spin, 3212, coords_to_world);
        let corfu = htw!(spin, 2602, coords_to_world);
        let andor = htw!(spin, 0236, coords_to_world);
        let candory = htw!(spin, 0336, coords_to_world);
        let reno = htw!(spin, 0102, coords_to_world);
        let regina = htw!(spin, 1910, coords_to_world);
        let mongo = htw!(spin, 1204, coords_to_world);
        let collace = htw!(spin, 1237, coords_to_world);
        let pavanne = htw!(spin, 2905, coords_to_world);
        let raweh = htw!(spin, 0139, coords_to_world);
        let javan = htw!(dene, 2131, coords_to_world);
        let salaam = htw!(dene, 3213, coords_to_world);

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
            world.populate_neighbors(&coords_to_world2, 3, false);
        }
        let mut sorted_coords: Vec<Coords>;
        sorted_coords = coords_to_world.keys().cloned().collect();
        assert_eq!(sorted_coords.len(), 825);
        sorted_coords.sort();
        for (ii, coords) in sorted_coords.iter_mut().enumerate() {
            let world = coords_to_world.get_mut(coords).unwrap();
            world.index = Some(ii);
        }
        let (dist2, pred2) =
            populate_navigable_distances(&sorted_coords, &coords_to_world, 2, false, ALG);
        let (dist3, pred3) =
            populate_navigable_distances(&sorted_coords, &coords_to_world, 3, false, ALG);

        let aramis = htw!(spin, 3110, coords_to_world);
        let ldd = htw!(spin, 3010, coords_to_world);
        let vinorian = htw!(spin, 3111, coords_to_world);
        let corfu = htw!(spin, 2602, coords_to_world);
        let andor = htw!(spin, 0236, coords_to_world);
        let candory = htw!(spin, 0336, coords_to_world);
        let reno = htw!(spin, 0102, coords_to_world);
        let mongo = htw!(spin, 1204, coords_to_world);
        let collace = htw!(spin, 1237, coords_to_world);
        let javan = htw!(dene, 2131, coords_to_world);
        let pysadi = htw!(spin, 3008, coords_to_world);
        let lewis = htw!(spin, 3107, coords_to_world);
        let yebab = htw!(spin, 3002, coords_to_world);
        let lablon = htw!(spin, 2701, coords_to_world);
        let violante = htw!(spin, 2708, coords_to_world);
        let focaline = htw!(spin, 2607, coords_to_world);
        let moughas = htw!(spin, 2406, coords_to_world);
        let enope = htw!(spin, 2205, coords_to_world);
        let becks_world = htw!(spin, 2204, coords_to_world);
        let yorbund = htw!(spin, 2303, coords_to_world);
        let heya = htw!(spin, 2402, coords_to_world);
        let zila = htw!(spin, 2908, coords_to_world);
        let zykoca = htw!(spin, 3004, coords_to_world);
        let feri = htw!(spin, 2005, coords_to_world);
        let uakye = htw!(spin, 1805, coords_to_world);
        let efate = htw!(spin, 1705, coords_to_world);
        let lysen = htw!(spin, 1307, coords_to_world);
        let nakege = htw!(spin, 1305, coords_to_world);

        let nutema = htw!(spin, 3112, coords_to_world);
        let celepina = htw!(spin, 2913, coords_to_world);
        let jae_tellona = htw!(spin, 2814, coords_to_world);
        let rhylanor = htw!(spin, 2716, coords_to_world);
        let equus = htw!(spin, 2417, coords_to_world);
        let ivendo = htw!(spin, 2319, coords_to_world);
        let quiru = htw!(spin, 2321, coords_to_world);
        let resten = htw!(spin, 2323, coords_to_world);
        let lunion = htw!(spin, 2124, coords_to_world);
        let derchon = htw!(spin, 2024, coords_to_world);
        let zaibon = htw!(spin, 1825, coords_to_world);
        let iron = htw!(spin, 1626, coords_to_world);
        let mithril = htw!(spin, 1628, coords_to_world);
        let steel = htw!(spin, 1529, coords_to_world);
        let dawnworld = htw!(spin, 1531, coords_to_world);
        let forine = htw!(spin, 1533, coords_to_world);
        let tarkine = htw!(spin, 1434, coords_to_world);
        let talos = htw!(spin, 1436, coords_to_world);

        let path = aramis
            .navigable_path(aramis, &sorted_coords, &coords_to_world, &dist2, &pred2)
            .unwrap();
        assert_eq!(path.len(), 1);
        assert_eq!(path[0], aramis.get_coords());

        let path = aramis
            .navigable_path(aramis, &sorted_coords, &coords_to_world, &dist3, &pred3)
            .unwrap();
        assert_eq!(path.len(), 1);
        assert_eq!(path[0], aramis.get_coords());

        let path = aramis
            .navigable_path(ldd, &sorted_coords, &coords_to_world, &dist2, &pred2)
            .unwrap();
        assert_eq!(path.len(), 2);
        assert_eq!(path[0], aramis.get_coords());
        assert_eq!(path[1], ldd.get_coords());

        let path = aramis
            .navigable_path(ldd, &sorted_coords, &coords_to_world, &dist3, &pred3)
            .unwrap();
        assert_eq!(path.len(), 2);
        assert_eq!(path[0], aramis.get_coords());
        assert_eq!(path[1], ldd.get_coords());

        let path = aramis
            .navigable_path(vinorian, &sorted_coords, &coords_to_world, &dist2, &pred2)
            .unwrap();
        assert_eq!(path.len(), 2);
        assert_eq!(path[0], aramis.get_coords());
        assert_eq!(path[1], vinorian.get_coords());

        let path = aramis
            .navigable_path(vinorian, &sorted_coords, &coords_to_world, &dist3, &pred3)
            .unwrap();
        assert_eq!(path.len(), 2);
        assert_eq!(path[0], aramis.get_coords());
        assert_eq!(path[1], vinorian.get_coords());

        let path = aramis
            .navigable_path(corfu, &sorted_coords, &coords_to_world, &dist2, &pred2)
            .unwrap();
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

        let path = aramis
            .navigable_path(corfu, &sorted_coords, &coords_to_world, &dist3, &pred3)
            .unwrap();
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

        let path = aramis
            .navigable_path(mongo, &sorted_coords, &coords_to_world, &dist2, &pred2)
            .unwrap();
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

        let path = aramis
            .navigable_path(collace, &sorted_coords, &coords_to_world, &dist2, &pred2)
            .unwrap();
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

        let path = reno
            .navigable_path(javan, &sorted_coords, &coords_to_world, &dist2, &pred2)
            .unwrap();
        for coords in &path {
            println!("{}", coords_to_world.get(&coords).unwrap().name);
        }
        assert_eq!(path.len(), 33);

        let path_opt =
            andor.navigable_path(candory, &sorted_coords, &coords_to_world, &dist2, &pred2);
        assert_eq!(path_opt, None);

        let path_opt =
            candory.navigable_path(andor, &sorted_coords, &coords_to_world, &dist2, &pred2);
        assert_eq!(path_opt, None);

        let path_opt =
            aramis.navigable_path(andor, &sorted_coords, &coords_to_world, &dist2, &pred2);
        assert_eq!(path_opt, None);

        let path = aramis
            .navigable_path(andor, &sorted_coords, &coords_to_world, &dist3, &pred3)
            .unwrap();
        for coords in &path {
            println!("{}", coords_to_world.get(&coords).unwrap().name);
        }
        assert_eq!(path.len(), 17);

        Ok(())
    }

    #[rstest]
    fn test_btn(data_dir: &PathBuf, download: &Result<Vec<String>>) -> Result<()> {
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
            world.populate_neighbors(&coords_to_world2, 3, false);
        }
        let mut sorted_coords: Vec<Coords>;
        sorted_coords = coords_to_world.keys().cloned().collect();
        sorted_coords.sort();
        assert_eq!(sorted_coords.len(), 825);
        for (ii, coords) in sorted_coords.iter_mut().enumerate() {
            let world = coords_to_world.get_mut(coords).unwrap();
            world.index = Some(ii);
        }
        let (dist2, _) =
            populate_navigable_distances(&sorted_coords, &coords_to_world, 2, false, ALG);

        let aramis = htw!(spin, 3110, coords_to_world);
        let ldd = htw!(spin, 3010, coords_to_world);
        let natoko = htw!(spin, 3209, coords_to_world);
        let vinorian = htw!(spin, 3111, coords_to_world);
        let corfu = htw!(spin, 2602, coords_to_world);
        let andor = htw!(spin, 0236, coords_to_world);
        let candory = htw!(spin, 0336, coords_to_world);
        let regina = htw!(spin, 1910, coords_to_world);
        let reacher = htw!(spin, 3210, coords_to_world);
        let nutema = htw!(spin, 3112, coords_to_world);
        let margesi = htw!(spin, 3212, coords_to_world);
        let saarinen = htw!(dene, 0113, coords_to_world);
        let lablon = htw!(spin, 2701, coords_to_world);
        let junidy = htw!(spin, 3202, coords_to_world);
        let marz = htw!(dene, 0201, coords_to_world);

        assert_eq!(aramis.btn(ldd, &dist2, false), 8.0);
        assert_eq!(aramis.btn(natoko, &dist2, false), 6.5);
        assert_eq!(aramis.btn(reacher, &dist2, false), 7.0);
        assert_eq!(aramis.btn(vinorian, &dist2, false), 8.0);
        assert_eq!(aramis.btn(corfu, &dist2, false), 5.5);
        assert_eq!(aramis.btn(lablon, &dist2, false), 6.0);
        assert_eq!(aramis.btn(junidy, &dist2, false), 7.5);
        assert_eq!(aramis.btn(marz, &dist2, false), 7.5);
        assert_eq!(aramis.btn(regina, &dist2, false), 7.0);
        assert_eq!(ldd.btn(aramis, &dist2, false), 8.0);
        assert_eq!(ldd.btn(natoko, &dist2, false), 6.0);
        assert_eq!(ldd.btn(reacher, &dist2, false), 6.5);
        assert_eq!(ldd.btn(nutema, &dist2, false), 6.0);
        assert_eq!(ldd.btn(margesi, &dist2, false), 6.0);
        assert_eq!(ldd.btn(saarinen, &dist2, false), 5.5);
        assert_eq!(natoko.btn(reacher, &dist2, false), 5.5);
        assert_eq!(vinorian.btn(nutema, &dist2, false), 6.5);
        assert_eq!(nutema.btn(margesi, &dist2, false), 5.5);
        assert_eq!(margesi.btn(saarinen, &dist2, false), 5.5);
        assert_eq!(aramis.btn(andor, &dist2, false), 0.0);
        assert_eq!(andor.btn(candory, &dist2, false), 0.0);
        Ok(())
    }

    #[rstest]
    fn test_passenger_btn(data_dir: &PathBuf, download: &Result<Vec<String>>) -> Result<()> {
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
            world.populate_neighbors(&coords_to_world2, 3, false);
        }
        let mut sorted_coords: Vec<Coords>;
        sorted_coords = coords_to_world.keys().cloned().collect();
        sorted_coords.sort();
        assert_eq!(sorted_coords.len(), 825);
        for (ii, coords) in sorted_coords.iter_mut().enumerate() {
            let world = coords_to_world.get_mut(coords).unwrap();
            world.index = Some(ii);
        }
        let (dist2, _) =
            populate_navigable_distances(&sorted_coords, &coords_to_world, 2, false, ALG);

        let aramis = htw!(spin, 3110, coords_to_world);
        let ldd = htw!(spin, 3010, coords_to_world);
        let natoko = htw!(spin, 3209, coords_to_world);
        let vinorian = htw!(spin, 3111, coords_to_world);
        let corfu = htw!(spin, 2602, coords_to_world);
        let andor = htw!(spin, 0236, coords_to_world);
        let candory = htw!(spin, 0336, coords_to_world);
        let regina = htw!(spin, 1910, coords_to_world);
        let reacher = htw!(spin, 3210, coords_to_world);
        let nutema = htw!(spin, 3112, coords_to_world);
        let margesi = htw!(spin, 3212, coords_to_world);
        let saarinen = htw!(dene, 0113, coords_to_world);
        let lablon = htw!(spin, 2701, coords_to_world);
        let junidy = htw!(spin, 3202, coords_to_world);
        let marz = htw!(dene, 0201, coords_to_world);

        assert_eq!(aramis.btn(ldd, &dist2, true), 8.5);
        assert_eq!(aramis.btn(natoko, &dist2, true), 7.0);
        assert_eq!(aramis.btn(reacher, &dist2, true), 7.5);
        assert_eq!(aramis.btn(vinorian, &dist2, true), 8.5);
        assert_eq!(aramis.btn(corfu, &dist2, true), 6.0);
        assert_eq!(aramis.btn(lablon, &dist2, true), 6.5);
        assert_eq!(aramis.btn(junidy, &dist2, true), 8.0);
        assert_eq!(aramis.btn(marz, &dist2, true), 8.0);
        assert_eq!(aramis.btn(regina, &dist2, true), 8.5);
        assert_eq!(ldd.btn(aramis, &dist2, true), 8.5);
        assert_eq!(ldd.btn(natoko, &dist2, true), 6.0);
        assert_eq!(ldd.btn(reacher, &dist2, true), 6.5);
        assert_eq!(ldd.btn(nutema, &dist2, true), 6.0);
        assert_eq!(ldd.btn(margesi, &dist2, true), 6.0);
        assert_eq!(ldd.btn(saarinen, &dist2, true), 5.5);
        assert_eq!(natoko.btn(reacher, &dist2, true), 5.5);
        assert_eq!(vinorian.btn(nutema, &dist2, true), 6.5);
        assert_eq!(nutema.btn(margesi, &dist2, true), 5.5);
        assert_eq!(margesi.btn(saarinen, &dist2, true), 5.5);
        assert_eq!(aramis.btn(andor, &dist2, true), 0.0);
        assert_eq!(andor.btn(candory, &dist2, true), 0.0);
        Ok(())
    }

    #[rstest]
    fn test_find_max_allowed_jump() {
        let mut max_jumps = HashMap::new();
        max_jumps.insert(Minor, 2);
        max_jumps.insert(Feeder, 3);
        max_jumps.insert(Intermediate, 3);
        max_jumps.insert(Main, 3);
        max_jumps.insert(Major, 3);

        assert_eq!(find_max_allowed_jump(0.0, &max_jumps, 8.0), 2);
        assert_eq!(find_max_allowed_jump(7.5, &max_jumps, 8.0), 2);
        assert_eq!(find_max_allowed_jump(8.0, &max_jumps, 8.0), 2);
        assert_eq!(find_max_allowed_jump(8.5, &max_jumps, 8.0), 2);
        assert_eq!(find_max_allowed_jump(9.0, &max_jumps, 8.0), 3);
        assert_eq!(find_max_allowed_jump(9.5, &max_jumps, 8.0), 3);
        assert_eq!(find_max_allowed_jump(10.0, &max_jumps, 8.0), 3);
        assert_eq!(find_max_allowed_jump(10.5, &max_jumps, 8.0), 3);

        max_jumps.insert(Minor, 1);
        max_jumps.insert(Feeder, 2);
        max_jumps.insert(Intermediate, 3);
        max_jumps.insert(Main, 4);
        max_jumps.insert(Major, 5);

        assert_eq!(find_max_allowed_jump(0.0, &max_jumps, 8.0), 1);
        assert_eq!(find_max_allowed_jump(7.5, &max_jumps, 8.0), 1);
        assert_eq!(find_max_allowed_jump(8.0, &max_jumps, 8.0), 1);
        assert_eq!(find_max_allowed_jump(8.5, &max_jumps, 8.0), 1);
        assert_eq!(find_max_allowed_jump(9.0, &max_jumps, 8.0), 2);
        assert_eq!(find_max_allowed_jump(9.5, &max_jumps, 8.0), 2);
        assert_eq!(find_max_allowed_jump(10.0, &max_jumps, 8.0), 3);
        assert_eq!(find_max_allowed_jump(10.5, &max_jumps, 8.0), 3);
        assert_eq!(find_max_allowed_jump(11.0, &max_jumps, 8.0), 4);
        assert_eq!(find_max_allowed_jump(11.5, &max_jumps, 8.0), 4);
        assert_eq!(find_max_allowed_jump(12.0, &max_jumps, 8.0), 5);
        assert_eq!(find_max_allowed_jump(12.5, &max_jumps, 8.0), 5);
    }

    #[rstest]
    fn test_populate_trade_routes(
        data_dir: &PathBuf,
        download: &Result<Vec<String>>,
    ) -> Result<()> {
        if let Ok(_sector_names) = download {};
        let mut coords_to_world: HashMap<Coords, World> = HashMap::new();
        let mut location_to_sector: HashMap<(i64, i64), Sector> = HashMap::new();
        let spin = Sector::new(
            &data_dir,
            "Spinward Marches".to_string(),
            &mut coords_to_world,
        );
        let dene = Sector::new(&data_dir, "Deneb".to_string(), &mut coords_to_world);
        let gvur = Sector::new(&data_dir, "Gvurrdon".to_string(), &mut coords_to_world);
        location_to_sector.insert(spin.location, spin.clone());
        location_to_sector.insert(dene.location, dene.clone());
        location_to_sector.insert(gvur.location, gvur.clone());
        for sector in location_to_sector.values() {
            sector
                .parse_xml_routes(&data_dir, &location_to_sector, &mut coords_to_world)
                .unwrap();
        }
        // Make a temporary clone to avoid having mutable and immutable refs.
        let coords_to_world2 = coords_to_world.clone();
        for world in coords_to_world.values_mut() {
            world.populate_neighbors(&coords_to_world2, 3, false);
        }
        let mut sorted_coords: Vec<Coords>;
        sorted_coords = coords_to_world.keys().cloned().collect();
        sorted_coords.sort();
        for (ii, coords) in sorted_coords.iter_mut().enumerate() {
            let world = coords_to_world.get_mut(coords).unwrap();
            world.index = Some(ii);
        }

        let mut max_jumps = HashMap::new();
        max_jumps.insert(Minor, 2);
        max_jumps.insert(Feeder, 3);
        max_jumps.insert(Intermediate, 3);
        max_jumps.insert(Main, 3);
        max_jumps.insert(Major, 3);
        let all_jumps: HashSet<u64> = max_jumps.values().cloned().collect();
        let mut dists: HashMap<u64, Array2<u16>> = HashMap::new();
        let mut preds: HashMap<u64, Array2<u16>> = HashMap::new();
        for jump in all_jumps.iter() {
            let (dist, pred) =
                populate_navigable_distances(&sorted_coords, &coords_to_world, *jump, false, ALG);
            dists.insert(*jump, dist);
            preds.insert(*jump, pred);
        }

        populate_trade_routes(
            &mut coords_to_world,
            *MIN_BTN,
            *MIN_ROUTE_BTN,
            false,
            &max_jumps,
            &dists,
            &preds,
        );

        let aramis = htw!(spin, 3110, coords_to_world);
        let mora = htw!(spin, 3124, coords_to_world);
        let jesedipere = htw!(spin, 3001, coords_to_world);
        let rruthaekuksu = htw!(gvur, 2840, coords_to_world);

        fn set_to_worlds(
            set: &HashSet<Coords>,
            coords_to_world: &HashMap<Coords, World>,
        ) -> Vec<String> {
            set.iter()
                .map(|&c| coords_to_world.get(&c).unwrap().name.clone())
                .collect()
        }

        println!(
            "aramis major {:?}",
            set_to_worlds(&aramis.major_routes, &coords_to_world)
        );
        println!(
            "aramis main {:?}",
            set_to_worlds(&aramis.main_routes, &coords_to_world)
        );
        println!(
            "aramis intermediate {:?}",
            set_to_worlds(&aramis.intermediate_routes, &coords_to_world)
        );
        println!(
            "aramis feeder {:?}",
            set_to_worlds(&aramis.feeder_routes, &coords_to_world)
        );
        println!(
            "aramis minor {:?}",
            set_to_worlds(&aramis.minor_routes, &coords_to_world)
        );
        assert_eq!(aramis.major_routes.len(), 0);
        assert_eq!(aramis.main_routes.len(), 0);
        assert_eq!(aramis.intermediate_routes.len(), 5);
        assert_eq!(aramis.feeder_routes.len(), 7);
        assert_eq!(aramis.minor_routes.len(), 1);

        println!(
            "mora major {:?}",
            set_to_worlds(&mora.major_routes, &coords_to_world)
        );
        println!(
            "mora main {:?}",
            set_to_worlds(&mora.main_routes, &coords_to_world)
        );
        println!(
            "mora intermediate {:?}",
            set_to_worlds(&mora.intermediate_routes, &coords_to_world)
        );
        println!(
            "mora feeder {:?}",
            set_to_worlds(&mora.feeder_routes, &coords_to_world)
        );
        println!(
            "mora minor {:?}",
            set_to_worlds(&mora.minor_routes, &coords_to_world)
        );
        assert_eq!(mora.major_routes.len(), 1);
        assert_eq!(mora.main_routes.len(), 8);
        assert_eq!(mora.intermediate_routes.len(), 4);
        assert_eq!(mora.feeder_routes.len(), 1);
        assert_eq!(mora.minor_routes.len(), 0);

        println!(
            "jesedipere major {:?}",
            set_to_worlds(&jesedipere.major_routes, &coords_to_world)
        );
        println!(
            "jesedipere main {:?}",
            set_to_worlds(&jesedipere.main_routes, &coords_to_world)
        );
        println!(
            "jesedipere intermediate {:?}",
            set_to_worlds(&jesedipere.intermediate_routes, &coords_to_world)
        );
        println!(
            "jesedipere feeder {:?}",
            set_to_worlds(&jesedipere.feeder_routes, &coords_to_world)
        );
        println!(
            "jesedipere minor {:?}",
            set_to_worlds(&jesedipere.minor_routes, &coords_to_world)
        );
        assert_eq!(jesedipere.major_routes.len(), 0);
        assert_eq!(jesedipere.main_routes.len(), 0);
        assert_eq!(jesedipere.intermediate_routes.len(), 2);
        assert_eq!(jesedipere.feeder_routes.len(), 5);
        assert_eq!(jesedipere.minor_routes.len(), 1);

        println!(
            "rruthaekuksu major {:?}",
            set_to_worlds(&rruthaekuksu.major_routes, &coords_to_world)
        );
        println!(
            "rruthaekuksu main {:?}",
            set_to_worlds(&rruthaekuksu.main_routes, &coords_to_world)
        );
        println!(
            "rruthaekuksu intermediate {:?}",
            set_to_worlds(&rruthaekuksu.intermediate_routes, &coords_to_world)
        );
        println!(
            "rruthaekuksu feeder {:?}",
            set_to_worlds(&rruthaekuksu.feeder_routes, &coords_to_world)
        );
        println!(
            "rruthaekuksu minor {:?}",
            set_to_worlds(&rruthaekuksu.minor_routes, &coords_to_world)
        );
        assert_eq!(rruthaekuksu.major_routes.len(), 0);
        assert_eq!(rruthaekuksu.main_routes.len(), 0);
        assert_eq!(rruthaekuksu.intermediate_routes.len(), 0);
        assert_eq!(rruthaekuksu.feeder_routes.len(), 4);
        assert_eq!(rruthaekuksu.minor_routes.len(), 0);

        Ok(())
    }

    #[rstest]
    fn test_port_size(data_dir: &PathBuf, download: &Result<Vec<String>>) -> Result<()> {
        if let Ok(_sector_names) = download {};
        let mut coords_to_world: HashMap<Coords, World> = HashMap::new();
        let mut location_to_sector: HashMap<(i64, i64), Sector> = HashMap::new();
        let spin = Sector::new(
            &data_dir,
            "Spinward Marches".to_string(),
            &mut coords_to_world,
        );
        let dene = Sector::new(&data_dir, "Deneb".to_string(), &mut coords_to_world);
        let gvur = Sector::new(&data_dir, "Gvurrdon".to_string(), &mut coords_to_world);
        location_to_sector.insert(spin.location, spin.clone());
        location_to_sector.insert(dene.location, dene.clone());
        location_to_sector.insert(gvur.location, gvur.clone());
        for sector in location_to_sector.values() {
            sector
                .parse_xml_routes(&data_dir, &location_to_sector, &mut coords_to_world)
                .unwrap();
        }
        // Make a temporary clone to avoid having mutable and immutable refs.
        let coords_to_world2 = coords_to_world.clone();
        for world in coords_to_world.values_mut() {
            world.populate_neighbors(&coords_to_world2, 3, false);
        }
        let mut sorted_coords: Vec<Coords>;
        sorted_coords = coords_to_world.keys().cloned().collect();
        sorted_coords.sort();
        for (ii, coords) in sorted_coords.iter_mut().enumerate() {
            let world = coords_to_world.get_mut(coords).unwrap();
            world.index = Some(ii);
        }

        let mut max_jumps = HashMap::new();
        max_jumps.insert(Minor, 2);
        max_jumps.insert(Feeder, 3);
        max_jumps.insert(Intermediate, 3);
        max_jumps.insert(Main, 3);
        max_jumps.insert(Major, 3);
        let all_jumps: HashSet<u64> = max_jumps.values().cloned().collect();
        let mut dists: HashMap<u64, Array2<u16>> = HashMap::new();
        let mut preds: HashMap<u64, Array2<u16>> = HashMap::new();
        for jump in all_jumps.iter() {
            let (dist, pred) =
                populate_navigable_distances(&sorted_coords, &coords_to_world, *jump, false, ALG);
            dists.insert(*jump, dist);
            preds.insert(*jump, pred);
        }
        populate_trade_routes(
            &mut coords_to_world,
            *MIN_BTN,
            *MIN_ROUTE_BTN,
            false,
            &max_jumps,
            &dists,
            &preds,
        );

        let aramis = htw!(spin, 3110, coords_to_world);
        let mora = htw!(spin, 3124, coords_to_world);
        let jesedipere = htw!(spin, 3001, coords_to_world);
        let rruthaekuksu = htw!(gvur, 2840, coords_to_world);

        assert_eq!(aramis.port_size(), 6);
        assert_eq!(mora.port_size(), 8);
        assert_eq!(jesedipere.port_size(), 5);
        assert_eq!(rruthaekuksu.port_size(), 5);

        Ok(())
    }

    #[rstest]
    fn test_islands(data_dir: &PathBuf, download: &Result<Vec<String>>) -> Result<()> {
        if let Ok(_sector_names) = download {};
        let mut coords_to_world: HashMap<Coords, World> = HashMap::new();
        let mut location_to_sector: HashMap<(i64, i64), Sector> = HashMap::new();
        let reft = Sector::new(&data_dir, "Reft".to_string(), &mut coords_to_world);
        location_to_sector.insert(reft.location, reft.clone());
        for sector in location_to_sector.values() {
            sector
                .parse_xml_routes(&data_dir, &location_to_sector, &mut coords_to_world)
                .unwrap();
        }
        // Make a temporary clone to avoid having mutable and immutable refs.
        let coords_to_world2 = coords_to_world.clone();
        for world in coords_to_world.values_mut() {
            world.populate_neighbors(&coords_to_world2, 3, false);
        }
        let mut sorted_coords: Vec<Coords>;
        sorted_coords = coords_to_world.keys().cloned().collect();
        sorted_coords.sort();
        for (ii, coords) in sorted_coords.iter_mut().enumerate() {
            let world = coords_to_world.get_mut(coords).unwrap();
            world.index = Some(ii);
        }

        let mut max_jumps = HashMap::new();
        max_jumps.insert(Minor, 2);
        max_jumps.insert(Feeder, 3);
        max_jumps.insert(Intermediate, 3);
        max_jumps.insert(Main, 3);
        max_jumps.insert(Major, 3);
        let all_jumps: HashSet<u64> = max_jumps.values().cloned().collect();
        let mut dists: HashMap<u64, Array2<u16>> = HashMap::new();
        let mut preds: HashMap<u64, Array2<u16>> = HashMap::new();
        for jump in all_jumps.iter() {
            let (dist, pred) =
                populate_navigable_distances(&sorted_coords, &coords_to_world, *jump, false, ALG);
            dists.insert(*jump, dist);
            preds.insert(*jump, pred);
        }
        populate_trade_routes(
            &mut coords_to_world,
            *MIN_BTN,
            *MIN_ROUTE_BTN,
            false,
            &max_jumps,
            &dists,
            &preds,
        );

        let dist2 = dists.get(&2).unwrap();
        let dist3 = dists.get(&3).unwrap();

        let zuflucht = htw!(reft, 0921, coords_to_world);
        let wellington = htw!(reft, 0925, coords_to_world);
        let esperanza = htw!(reft, 0926, coords_to_world);
        let st_hilaire = htw!(reft, 0930, coords_to_world);
        let nebelwelt = htw!(reft, 1030, coords_to_world);
        let gloire = htw!(reft, 1123, coords_to_world);
        let serendip_belt = htw!(reft, 1323, coords_to_world);
        let new_colchis = htw!(reft, 1327, coords_to_world);
        let herzenslust = htw!(reft, 1426, coords_to_world);
        let orphee = htw!(reft, 1429, coords_to_world);
        let topas = htw!(reft, 1522, coords_to_world);
        let elysee = htw!(reft, 1525, coords_to_world);
        let besancon = htw!(reft, 1526, coords_to_world);
        let berlichingen = htw!(reft, 1621, coords_to_world);
        let joyeuse = htw!(reft, 1628, coords_to_world);
        let sturgeons_law = htw!(reft, 1724, coords_to_world);
        let quichotte = htw!(reft, 1729, coords_to_world);
        let neubayern = htw!(reft, 1822, coords_to_world);
        let schlesien_belt = htw!(reft, 1923, coords_to_world);
        let new_home = htw!(reft, 1925, coords_to_world);
        let colchis = htw!(reft, 2026, coords_to_world);
        let st_genevieve = htw!(reft, 2123, coords_to_world);
        let acadie = htw!(reft, 2225, coords_to_world);
        let sansterre = htw!(reft, 2322, coords_to_world);
        let achille = htw!(reft, 2324, coords_to_world);
        let amondiage = htw!(reft, 2325, coords_to_world);
        let st_denis = htw!(reft, 2423, coords_to_world);

        assert_eq!(zuflucht.uwtn(), 4.0);
        assert_eq!(zuflucht.wtn_port_modifier(), 0.0);
        assert_eq!(zuflucht.wtn(), 4.0);
        assert_eq!(wellington.uwtn(), 2.0);
        assert_eq!(wellington.wtn_port_modifier(), 0.5);
        assert_eq!(wellington.wtn(), 2.5);
        assert_eq!(esperanza.uwtn(), 6.0);
        assert_eq!(esperanza.wtn_port_modifier(), 0.0);
        assert_eq!(esperanza.wtn(), 6.0);
        assert_eq!(st_hilaire.uwtn(), 4.5);
        assert_eq!(st_hilaire.wtn_port_modifier(), 0.0);
        assert_eq!(st_hilaire.wtn(), 4.5);
        assert_eq!(nebelwelt.uwtn(), 2.5);
        assert_eq!(nebelwelt.wtn_port_modifier(), 0.5);
        assert_eq!(nebelwelt.wtn(), 3.0);
        assert_eq!(gloire.uwtn(), 3.5);
        assert_eq!(gloire.wtn_port_modifier(), 0.0);
        assert_eq!(gloire.wtn(), 3.5);
        assert_eq!(serendip_belt.uwtn(), 5.5);
        assert_eq!(serendip_belt.wtn_port_modifier(), 0.0);
        assert_eq!(serendip_belt.wtn(), 5.5);
        assert_eq!(new_colchis.uwtn(), 5.5);
        assert_eq!(new_colchis.wtn_port_modifier(), 0.0);
        assert_eq!(new_colchis.wtn(), 5.5);
        assert_eq!(herzenslust.uwtn(), 4.0);
        assert_eq!(herzenslust.wtn_port_modifier(), -1.0);
        assert_eq!(herzenslust.wtn(), 3.0);
        assert_eq!(orphee.uwtn(), 2.5);
        assert_eq!(orphee.wtn_port_modifier(), -2.5);
        assert_eq!(orphee.wtn(), 0.0);
        assert_eq!(topas.uwtn(), 4.5);
        assert_eq!(topas.wtn_port_modifier(), -0.5);
        assert_eq!(topas.wtn(), 4.0);
        assert_eq!(elysee.uwtn(), 3.5);
        assert_eq!(elysee.wtn_port_modifier(), 0.5);
        assert_eq!(elysee.wtn(), 4.0);
        assert_eq!(besancon.uwtn(), 2.5);
        assert_eq!(besancon.wtn_port_modifier(), 0.5);
        assert_eq!(besancon.wtn(), 3.0);
        assert_eq!(berlichingen.uwtn(), 2.5);
        assert_eq!(berlichingen.wtn_port_modifier(), 0.0);
        assert_eq!(berlichingen.wtn(), 2.5);
        assert_eq!(joyeuse.uwtn(), 5.5);
        assert_eq!(joyeuse.wtn_port_modifier(), 0.0);
        assert_eq!(joyeuse.wtn(), 5.5);
        assert_eq!(sturgeons_law.uwtn(), 3.0);
        assert_eq!(sturgeons_law.wtn_port_modifier(), 0.0);
        assert_eq!(sturgeons_law.wtn(), 3.0);
        assert_eq!(quichotte.uwtn(), 3.5);
        assert_eq!(quichotte.wtn_port_modifier(), -0.5);
        assert_eq!(quichotte.wtn(), 3.0);
        assert_eq!(neubayern.uwtn(), 5.5);
        assert_eq!(neubayern.wtn_port_modifier(), 0.0);
        assert_eq!(neubayern.wtn(), 5.5);
        assert_eq!(schlesien_belt.uwtn(), 2.5);
        assert_eq!(schlesien_belt.wtn_port_modifier(), 0.5);
        assert_eq!(schlesien_belt.wtn(), 3.0);
        assert_eq!(new_home.uwtn(), 5.0);
        assert_eq!(new_home.wtn_port_modifier(), 0.0);
        assert_eq!(new_home.wtn(), 5.0);
        assert_eq!(colchis.uwtn(), 5.0);
        assert_eq!(colchis.wtn_port_modifier(), 0.0);
        assert_eq!(colchis.wtn(), 5.0);
        assert_eq!(st_genevieve.uwtn(), 1.5);
        assert_eq!(st_genevieve.wtn_port_modifier(), 0.5);
        assert_eq!(st_genevieve.wtn(), 2.0);
        assert_eq!(acadie.uwtn(), 3.5);
        assert_eq!(acadie.wtn_port_modifier(), 0.0);
        assert_eq!(acadie.wtn(), 3.5);
        assert_eq!(sansterre.uwtn(), 5.5);
        assert_eq!(sansterre.wtn_port_modifier(), 0.0);
        assert_eq!(sansterre.wtn(), 5.5);
        assert_eq!(achille.uwtn(), 2.5);
        assert_eq!(achille.wtn_port_modifier(), 0.0);
        assert_eq!(achille.wtn(), 2.5);
        assert_eq!(amondiage.uwtn(), 5.5);
        assert_eq!(amondiage.wtn_port_modifier(), 0.0);
        assert_eq!(amondiage.wtn(), 5.5);
        assert_eq!(st_denis.uwtn(), 4.0);
        assert_eq!(st_denis.wtn_port_modifier(), -0.5);
        assert_eq!(st_denis.wtn(), 3.5);

        assert!(zuflucht.neighbors[1].is_empty());
        assert!(zuflucht.neighbors[2].is_empty());
        assert_eq!(zuflucht.neighbors[3].len(), 1);
        assert_eq!(
            zuflucht.neighbors[3].iter().next(),
            Some(&gloire.get_coords())
        );
        assert!(zuflucht.xboat_routes.is_empty());

        assert_eq!(zuflucht.navigable_distance(wellington, &dist2), INFINITY);
        assert_eq!(zuflucht.btn(wellington, &dist2, false), 0.0);
        assert_eq!(zuflucht.btn(wellington, &dist2, true), 0.0);
        assert_eq!(zuflucht.navigable_distance(wellington, &dist3), 6);
        assert_eq!(zuflucht.wtcm(wellington), -0.5);
        assert_eq!(zuflucht.btn(wellington, &dist3, false), 4.5);
        assert_eq!(zuflucht.btn(wellington, &dist3, true), 4.5);

        assert_eq!(zuflucht.navigable_distance(esperanza, &dist2), INFINITY);
        assert_eq!(zuflucht.wtcm(esperanza), -0.5);
        assert_eq!(zuflucht.btn(esperanza, &dist2, false), 0.0);
        assert_eq!(zuflucht.btn(esperanza, &dist2, true), 0.0);
        assert_eq!(zuflucht.navigable_distance(esperanza, &dist3), 7);
        assert_eq!(zuflucht.btn(esperanza, &dist3, false), 8.0);
        assert_eq!(zuflucht.btn(esperanza, &dist3, true), 8.0);

        assert_eq!(zuflucht.navigable_distance(gloire, &dist2), INFINITY);
        assert_eq!(zuflucht.wtcm(gloire), -0.5);
        assert_eq!(zuflucht.btn(gloire, &dist2, false), 0.0);
        assert_eq!(zuflucht.btn(gloire, &dist2, true), 0.0);
        assert_eq!(zuflucht.navigable_distance(gloire, &dist3), 3);
        assert_eq!(zuflucht.btn(gloire, &dist3, false), 6.0);
        assert_eq!(zuflucht.btn(gloire, &dist3, true), 6.0);

        assert_eq!(zuflucht.navigable_distance(serendip_belt, &dist3), 5);
        assert_eq!(zuflucht.wtcm(serendip_belt), 0.0);
        assert_eq!(zuflucht.btn(serendip_belt, &dist3, false), 8.5);
        assert_eq!(zuflucht.btn(serendip_belt, &dist3, true), 8.5);

        assert_eq!(zuflucht.navigable_distance(herzenslust, &dist3), 10);
        assert_eq!(zuflucht.wtcm(herzenslust), -0.5);
        assert_eq!(zuflucht.btn(herzenslust, &dist3, false), 4.5);
        assert_eq!(zuflucht.btn(herzenslust, &dist3, true), 4.5);

        assert_eq!(zuflucht.navigable_distance(orphee, &dist3), INFINITY);
        assert_eq!(zuflucht.wtcm(orphee), -0.5);
        assert_eq!(zuflucht.btn(orphee, &dist3, false), 0.0);
        assert_eq!(zuflucht.btn(orphee, &dist3, true), 0.0);

        assert_eq!(zuflucht.navigable_distance(topas, &dist3), 7);
        assert_eq!(zuflucht.wtcm(topas), 0.0);
        assert_eq!(zuflucht.btn(topas, &dist3, false), 6.5);
        assert_eq!(zuflucht.btn(topas, &dist3, true), 6.5);

        assert_eq!(zuflucht.navigable_distance(elysee, &dist3), 8);
        assert_eq!(zuflucht.wtcm(elysee), -0.5);
        assert_eq!(zuflucht.btn(elysee, &dist3, false), 6.0);
        assert_eq!(zuflucht.btn(elysee, &dist3, true), 6.0);

        assert_eq!(zuflucht.navigable_distance(besancon, &dist3), 9);
        assert_eq!(zuflucht.wtcm(besancon), -0.5);
        assert_eq!(zuflucht.btn(besancon, &dist3, false), 5.0);
        assert_eq!(zuflucht.btn(besancon, &dist3, true), 5.0);

        assert_eq!(zuflucht.navigable_distance(berlichingen, &dist3), 8);
        assert_eq!(zuflucht.wtcm(berlichingen), -0.5);
        assert_eq!(zuflucht.btn(berlichingen, &dist3, false), 4.5);
        assert_eq!(zuflucht.btn(berlichingen, &dist3, true), 4.5);

        assert_eq!(zuflucht.navigable_distance(joyeuse, &dist3), 12);
        assert_eq!(zuflucht.wtcm(joyeuse), -0.5);
        assert_eq!(zuflucht.btn(joyeuse, &dist3, false), 7.0);
        assert_eq!(zuflucht.btn(joyeuse, &dist3, true), 7.0);

        assert_eq!(zuflucht.navigable_distance(sturgeons_law, &dist3), 10);
        assert_eq!(zuflucht.wtcm(sturgeons_law), -0.5);
        assert_eq!(zuflucht.btn(sturgeons_law, &dist3, false), 4.5);
        assert_eq!(zuflucht.btn(sturgeons_law, &dist3, true), 4.5);

        assert_eq!(zuflucht.navigable_distance(quichotte, &dist3), 13);
        assert_eq!(zuflucht.wtcm(quichotte), -0.5);
        assert_eq!(zuflucht.btn(quichotte, &dist3, false), 4.5);
        assert_eq!(zuflucht.btn(quichotte, &dist3, true), 4.5);

        assert_eq!(zuflucht.navigable_distance(neubayern, &dist3), 10);
        assert_eq!(zuflucht.wtcm(neubayern), -0.5);
        assert_eq!(zuflucht.btn(neubayern, &dist3, false), 7.0);
        assert_eq!(zuflucht.btn(neubayern, &dist3, true), 7.0);

        assert_eq!(zuflucht.navigable_distance(schlesien_belt, &dist3), 11);
        assert_eq!(zuflucht.wtcm(schlesien_belt), -0.5);
        assert_eq!(zuflucht.btn(schlesien_belt, &dist3, false), 4.5);
        assert_eq!(zuflucht.btn(schlesien_belt, &dist3, true), 4.5);

        assert_eq!(zuflucht.navigable_distance(new_home, &dist3), 12);
        assert_eq!(zuflucht.wtcm(new_home), -0.5);
        assert_eq!(zuflucht.btn(new_home, &dist3, false), 6.5);
        assert_eq!(zuflucht.btn(new_home, &dist3, true), 7.0);

        assert_eq!(zuflucht.navigable_distance(colchis, &dist3), 14);
        assert_eq!(zuflucht.wtcm(colchis), -0.5);
        assert_eq!(zuflucht.btn(colchis, &dist3, false), 6.5);
        assert_eq!(zuflucht.btn(colchis, &dist3, true), 6.5);

        assert_eq!(zuflucht.navigable_distance(st_genevieve, &dist3), 13);
        assert_eq!(zuflucht.wtcm(st_genevieve), -0.5);
        assert_eq!(zuflucht.btn(st_genevieve, &dist3, false), 3.5);
        assert_eq!(zuflucht.btn(st_genevieve, &dist3, true), 3.5);

        assert_eq!(zuflucht.navigable_distance(acadie, &dist3), 15);
        assert_eq!(zuflucht.wtcm(acadie), -0.5);
        assert_eq!(zuflucht.btn(acadie, &dist3, false), 5.0);
        assert_eq!(zuflucht.btn(acadie, &dist3, true), 5.0);

        assert_eq!(zuflucht.navigable_distance(sansterre, &dist3), 15);
        assert_eq!(zuflucht.wtcm(sansterre), -0.5);
        assert_eq!(zuflucht.btn(sansterre, &dist3, false), 7.0);
        assert_eq!(zuflucht.btn(sansterre, &dist3, true), 7.0);

        assert_eq!(zuflucht.navigable_distance(achille, &dist3), 15);
        assert_eq!(zuflucht.wtcm(achille), -0.5);
        assert_eq!(zuflucht.btn(achille, &dist3, false), 4.0);
        assert_eq!(zuflucht.btn(achille, &dist3, true), 4.0);

        assert_eq!(zuflucht.navigable_distance(amondiage, &dist3), 16);
        assert_eq!(zuflucht.wtcm(amondiage), -0.5);
        assert_eq!(zuflucht.btn(amondiage, &dist3, false), 7.0);
        assert_eq!(zuflucht.btn(amondiage, &dist3, true), 7.0);

        assert_eq!(zuflucht.navigable_distance(st_denis, &dist3), 16);
        assert_eq!(zuflucht.wtcm(st_denis), -0.5);
        assert_eq!(zuflucht.btn(st_denis, &dist3, false), 5.0);
        assert_eq!(zuflucht.btn(st_denis, &dist3, true), 5.0);

        println!("{:?}", zuflucht.dbtn_to_coords);
        for (ii, set) in zuflucht.dbtn_to_coords.iter().enumerate() {
            print!("dbtn {} ", ii);
            for coords in set {
                println!("{}", coords_to_world.get(&coords).unwrap().name);
            }
        }
        assert_eq!(zuflucht.dbtn_to_coords[0].len(), 6);
        for ii in 1..12 {
            assert!(zuflucht.dbtn_to_coords[ii].is_empty());
        }
        assert_eq!(zuflucht.dbtn_to_coords[13].len(), 3);
        assert_eq!(zuflucht.dbtn_to_coords[14].len(), 5);
        assert!(zuflucht.dbtn_to_coords[15].is_empty());
        assert_eq!(zuflucht.dbtn_to_coords[16].len(), 1);
        assert_eq!(
            zuflucht.dbtn_to_coords[16].iter().next(),
            Some(&esperanza.get_coords())
        );
        assert_eq!(zuflucht.dbtn_to_coords[17].len(), 1);
        assert_eq!(
            zuflucht.dbtn_to_coords[17].iter().next(),
            Some(&serendip_belt.get_coords())
        );
        for ii in 18..zuflucht.dbtn_to_coords.len() - 1 {
            assert!(zuflucht.dbtn_to_coords[ii].is_empty());
        }
        assert_eq!(zuflucht.endpoint_trade_credits, 1_222_500_000);
        assert_eq!(zuflucht.transient_trade_credits, 0);

        Ok(())
    }

    #[rstest]
    fn test_generate_pdfs(
        data_dir: &PathBuf,
        output_dir: &PathBuf,
        download: &Result<Vec<String>>,
    ) -> Result<()> {
        if let Ok(_sector_names) = download {};
        let mut coords_to_world: HashMap<Coords, World> = HashMap::new();
        let mut location_to_sector: HashMap<(i64, i64), Sector> = HashMap::new();
        let spin = Sector::new(
            &data_dir,
            "Spinward Marches".to_string(),
            &mut coords_to_world,
        );
        let dene = Sector::new(&data_dir, "Deneb".to_string(), &mut coords_to_world);
        let gvur = Sector::new(&data_dir, "Gvurrdon".to_string(), &mut coords_to_world);
        location_to_sector.insert(spin.location, spin.clone());
        location_to_sector.insert(dene.location, dene.clone());
        location_to_sector.insert(gvur.location, gvur.clone());
        for sector in location_to_sector.values() {
            sector
                .parse_xml_routes(&data_dir, &location_to_sector, &mut coords_to_world)
                .unwrap();
        }
        // Make a temporary clone to avoid having mutable and immutable refs.
        let coords_to_world2 = coords_to_world.clone();
        for world in coords_to_world.values_mut() {
            world.populate_neighbors(&coords_to_world2, 3, false);
        }
        let mut sorted_coords: Vec<Coords>;
        sorted_coords = coords_to_world.keys().cloned().collect();
        sorted_coords.sort();
        for (ii, coords) in sorted_coords.iter_mut().enumerate() {
            let world = coords_to_world.get_mut(coords).unwrap();
            world.index = Some(ii);
        }

        let mut max_jumps = HashMap::new();
        max_jumps.insert(Minor, 2);
        max_jumps.insert(Feeder, 3);
        max_jumps.insert(Intermediate, 3);
        max_jumps.insert(Main, 3);
        max_jumps.insert(Major, 3);
        let all_jumps: HashSet<u64> = max_jumps.values().cloned().collect();
        let mut dists: HashMap<u64, Array2<u16>> = HashMap::new();
        let mut preds: HashMap<u64, Array2<u16>> = HashMap::new();
        for jump in all_jumps.iter() {
            let (dist, pred) =
                populate_navigable_distances(&sorted_coords, &coords_to_world, *jump, false, ALG);
            dists.insert(*jump, dist);
            preds.insert(*jump, pred);
        }
        populate_trade_routes(
            &mut coords_to_world,
            *MIN_BTN,
            *MIN_ROUTE_BTN,
            false,
            &max_jumps,
            &dists,
            &preds,
        );

        let found_filename_results: Vec<Result<OsString, io::Error>> = read_dir(&output_dir)?
            .map(|res| res.map(|e| e.file_name()))
            .collect();
        assert_eq!(found_filename_results.len(), 0);
        generate_pdfs(output_dir, &location_to_sector, &coords_to_world);
        let found_filename_results: Vec<Result<OsString, io::Error>> = read_dir(&output_dir)?
            .map(|res| res.map(|e| e.file_name()))
            .collect();
        assert_eq!(found_filename_results.len(), 3);
        // TODO Validate the PDF files with pdf-rs

        Ok(())
    }
}
