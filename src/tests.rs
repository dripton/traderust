use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::fs::create_dir_all;
use std::path::PathBuf;

use crate::apsp::INFINITY;
use crate::{
    distance_modifier_table, download_sector_data, parse_header_and_separator,
    populate_navigable_distances, populate_trade_routes,
};
use crate::{Coords, Sector, World};

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
        assert_eq!(distance_modifier_table(999999), 6.0);
        assert_eq!(distance_modifier_table(INFINITY), 6.0);
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

        assert_eq!(aramis.distance_modifier(aramis, &dist2), 0.0);
        assert_eq!(aramis.distance_modifier(ldd, &dist2), 0.0);
        assert_eq!(aramis.distance_modifier(vinorian, &dist2), 0.0);
        assert_eq!(aramis.distance_modifier(corfu, &dist2), 2.0);
        assert_eq!(aramis.distance_modifier(andor, &dist2), 6.0);
        assert_eq!(aramis.distance_modifier(margesi, &dist2), 1.0);
        assert_eq!(aramis.distance_modifier(pavanne, &dist2), 1.5);
        assert_eq!(aramis.distance_modifier(regina, &dist2), 2.0);
        assert_eq!(aramis.distance_modifier(mongo, &dist2), 2.5);
        assert_eq!(aramis.distance_modifier(collace, &dist2), 3.0);
        assert_eq!(reno.distance_modifier(javan, &dist2), 3.5);
        assert_eq!(andor.distance_modifier(candory, &dist2), 6.0);
        assert_eq!(candory.distance_modifier(andor, &dist2), 6.0);
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
        let corfu = spin
            .hex_to_world("2602".to_string(), &coords_to_world)
            .unwrap();
        let andor = spin
            .hex_to_world("0236".to_string(), &coords_to_world)
            .unwrap();
        let candory = spin
            .hex_to_world("0336".to_string(), &coords_to_world)
            .unwrap();
        let regina = spin
            .hex_to_world("1910".to_string(), &coords_to_world)
            .unwrap();
        let reacher = spin
            .hex_to_world("3210".to_string(), &coords_to_world)
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
        let lablon = spin
            .hex_to_world("2701".to_string(), &coords_to_world)
            .unwrap();
        let junidy = spin
            .hex_to_world("3202".to_string(), &coords_to_world)
            .unwrap();
        let marz = dene
            .hex_to_world("0201".to_string(), &coords_to_world)
            .unwrap();

        assert_eq!(aramis.btn(ldd, &dist2), 8.0);
        assert_eq!(aramis.btn(natoko, &dist2), 6.5);
        assert_eq!(aramis.btn(reacher, &dist2), 7.0);
        assert_eq!(aramis.btn(vinorian, &dist2), 8.0);
        assert_eq!(aramis.btn(corfu, &dist2), 5.5);
        assert_eq!(aramis.btn(lablon, &dist2), 6.0);
        assert_eq!(aramis.btn(junidy, &dist2), 7.5);
        assert_eq!(aramis.btn(marz, &dist2), 7.5);
        assert_eq!(aramis.btn(regina, &dist2), 7.0);
        assert_eq!(ldd.btn(aramis, &dist2), 8.0);
        assert_eq!(ldd.btn(natoko, &dist2), 6.0);
        assert_eq!(ldd.btn(reacher, &dist2), 6.5);
        assert_eq!(ldd.btn(nutema, &dist2), 6.0);
        assert_eq!(ldd.btn(margesi, &dist2), 6.0);
        assert_eq!(ldd.btn(saarinen, &dist2), 5.5);
        assert_eq!(natoko.btn(reacher, &dist2), 5.5);
        assert_eq!(vinorian.btn(nutema, &dist2), 6.5);
        assert_eq!(nutema.btn(margesi, &dist2), 5.5);
        assert_eq!(margesi.btn(saarinen, &dist2), 5.5);
        assert_eq!(aramis.btn(andor, &dist2), 2.5);
        assert_eq!(andor.btn(candory, &dist2), 2.0);
        Ok(())
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
            world.populate_neighbors(&coords_to_world2);
        }
        let mut sorted_coords: Vec<Coords>;
        sorted_coords = coords_to_world.keys().cloned().collect();
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

        populate_trade_routes(
            &mut coords_to_world,
            &coords_to_index,
            &sorted_coords,
            &dist2,
            &pred2,
            &dist3,
            &pred3,
        );

        let aramis = spin
            .hex_to_world("3110".to_string(), &coords_to_world)
            .unwrap();
        let mora = spin
            .hex_to_world("3124".to_string(), &coords_to_world)
            .unwrap();
        let jesedipere = spin
            .hex_to_world("3001".to_string(), &coords_to_world)
            .unwrap();
        let rruthaekuksu = gvur
            .hex_to_world("2840".to_string(), &coords_to_world)
            .unwrap();

        fn set_to_worlds(
            set: &HashSet<Coords>,
            coords_to_world: &HashMap<Coords, World>,
        ) -> Vec<String> {
            set.iter()
                .map(|&c| coords_to_world.get(&c).unwrap().name.clone())
                .collect()
        }

        assert_eq!(aramis.major_routes.len(), 0);
        assert_eq!(aramis.main_routes.len(), 0);
        assert_eq!(aramis.intermediate_routes.len(), 5); // py 4
        assert_eq!(aramis.feeder_routes.len(), 7); // py 9
        assert_eq!(aramis.minor_routes.len(), 1); // py 0

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

        assert_eq!(mora.major_routes.len(), 1);
        assert_eq!(mora.main_routes.len(), 8);
        assert_eq!(mora.intermediate_routes.len(), 4); // py 5
        assert_eq!(mora.feeder_routes.len(), 1); // py 0
        assert_eq!(mora.minor_routes.len(), 0);

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

        assert_eq!(jesedipere.major_routes.len(), 0);
        assert_eq!(jesedipere.main_routes.len(), 0);
        assert_eq!(jesedipere.intermediate_routes.len(), 2); // py 0
        assert_eq!(jesedipere.feeder_routes.len(), 2); // py 3
        assert_eq!(jesedipere.minor_routes.len(), 2);

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

        assert_eq!(rruthaekuksu.major_routes.len(), 0);
        assert_eq!(rruthaekuksu.main_routes.len(), 0);
        assert_eq!(rruthaekuksu.intermediate_routes.len(), 0);
        assert_eq!(rruthaekuksu.feeder_routes.len(), 4);
        assert_eq!(rruthaekuksu.minor_routes.len(), 0); // py 2

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

        Ok(())
    }
}
