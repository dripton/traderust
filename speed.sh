#!/bin/bash

set -x

cargo build -r

time target/release/traderust -d /var/tmp/traderoutes/ -o /var/tmp/traderust_output -s 'Spinward Marches' -vvv
time target/release/traderust -d /var/tmp/traderoutes/ -o /var/tmp/traderust_output -s 'Spinward Marches' -s Deneb -vvv
time target/release/traderust -d /var/tmp/traderoutes/ -o /var/tmp/traderust_output -s 'Spinward Marches' -s Deneb -s Gvurrdon -vvv
time target/release/traderust -d /var/tmp/traderoutes/ -o /var/tmp/traderust_output -s 'Spinward Marches' -s Deneb -s Gvurrdon -s Tuglikki -vvv
time target/release/traderust -d /var/tmp/traderoutes/ -o /var/tmp/traderust_output -s 'Spinward Marches' -s Deneb -s Gvurrdon -s Tuglikki -s Provence -vvv
time target/release/traderust -d /var/tmp/traderoutes/ -o /var/tmp/traderust_output -s 'Spinward Marches' -s Deneb -s Gvurrdon -s Tuglikki -s Provence -s Corridor -vvv
time target/release/traderust -d /var/tmp/traderoutes/ -o /var/tmp/traderust_output -s 'Spinward Marches' -s Deneb -s Gvurrdon -s Tuglikki -s Provence -s Corridor -s Windhorn -vvv
time target/release/traderust -d /var/tmp/traderoutes/ -o /var/tmp/traderust_output -s 'Spinward Marches' -s Deneb -s Gvurrdon -s Tuglikki -s Provence -s Corridor -s Windhorn -s Vland -vvv
time target/release/traderust -d /var/tmp/traderoutes/ -o /var/tmp/traderust_output -s 'Spinward Marches' -s Deneb -s Gvurrdon -s Tuglikki -s Provence -s Corridor -s Windhorn -s Vland -s Meshan -vvv
time target/release/traderust -d /var/tmp/traderoutes/ -o /var/tmp/traderust_output -s 'Spinward Marches' -s Deneb -s Gvurrdon -s Tuglikki -s Provence -s Corridor -s Windhorn -s Vland -s Meshan -s Lishun -vvv
time target/release/traderust -d /var/tmp/traderoutes/ -o /var/tmp/traderust_output -s 'Spinward Marches' -s Deneb -s Gvurrdon -s Tuglikki -s Provence -s Corridor -s Windhorn -s Vland -s Meshan -s Lishun -s 'Trojan Reach' -vvv
time target/release/traderust -d /var/tmp/traderoutes/ -o /var/tmp/traderust_output -s 'Spinward Marches' -s Deneb -s Gvurrdon -s Tuglikki -s Provence -s Corridor -s Windhorn -s Vland -s Meshan -s Lishun -s 'Trojan Reach' -s Reft -vvv
time target/release/traderust -d /var/tmp/traderoutes/ -o /var/tmp/traderust_output -s 'Spinward Marches' -s Deneb -s Gvurrdon -s Tuglikki -s Provence -s Corridor -s Windhorn -s Vland -s Meshan -s Lishun -s 'Trojan Reach' -s Reft -s Gushemege -vvv
time target/release/traderust -d /var/tmp/traderoutes/ -o /var/tmp/traderust_output -s 'Spinward Marches' -s Deneb -s Gvurrdon -s Tuglikki -s Provence -s Corridor -s Windhorn -s Vland -s Meshan -s Lishun -s 'Trojan Reach' -s Reft -s Gushemege -s Dagudashaag -vvv
time target/release/traderust -d /var/tmp/traderoutes/ -o /var/tmp/traderust_output -s 'Spinward Marches' -s Deneb -s Gvurrdon -s Tuglikki -s Provence -s Corridor -s Windhorn -s Vland -s Meshan -s Lishun -s 'Trojan Reach' -s Reft -s Gushemege -s Dagudashaag -s Core -vvv
time target/release/traderust -d /var/tmp/traderoutes/ -o /var/tmp/traderust_output -s 'Spinward Marches' -s Deneb -s Gvurrdon -s Tuglikki -s Provence -s Corridor -s Windhorn -s Vland -s Meshan -s Lishun -s 'Trojan Reach' -s Reft -s Gushemege -s Dagudashaag -s Core -s "Riftspan Reaches" -vvv
time target/release/traderust -d /var/tmp/traderoutes/ -o /var/tmp/traderust_output -s 'Spinward Marches' -s Deneb -s Gvurrdon -s Tuglikki -s Provence -s Corridor -s Windhorn -s Vland -s Meshan -s Lishun -s 'Trojan Reach' -s Reft -s Gushemege -s Dagudashaag -s Core -s "Riftspan Reaches" -s "Verge" -vvv
time target/release/traderust -d /var/tmp/traderoutes/ -o /var/tmp/traderust_output -s 'Spinward Marches' -s Deneb -s Gvurrdon -s Tuglikki -s Provence -s Corridor -s Windhorn -s Vland -s Meshan -s Lishun -s 'Trojan Reach' -s Reft -s Gushemege -s Dagudashaag -s Core -s "Riftspan Reaches" -s "Verge" -s "Ilelish" -vvv
time target/release/traderust -d /var/tmp/traderoutes/ -o /var/tmp/traderust_output -s 'Spinward Marches' -s Deneb -s Gvurrdon -s Tuglikki -s Provence -s Corridor -s Windhorn -s Vland -s Meshan -s Lishun -s 'Trojan Reach' -s Reft -s Gushemege -s Dagudashaag -s Core -s "Riftspan Reaches" -s "Verge" -s "Ilelish" -s "Zarushagar" -vvv
time target/release/traderust -d /var/tmp/traderoutes/ -o /var/tmp/traderust_output -s 'Spinward Marches' -s Deneb -s Gvurrdon -s Tuglikki -s Provence -s Corridor -s Windhorn -s Vland -s Meshan -s Lishun -s 'Trojan Reach' -s Reft -s Gushemege -s Dagudashaag -s Core -s "Riftspan Reaches" -s "Verge" -s "Ilelish" -s "Zarushagar" -s Massilia -vvv
time target/release/traderust -d /var/tmp/traderoutes/ -o /var/tmp/traderust_output -s 'Spinward Marches' -s Deneb -s Gvurrdon -s Tuglikki -s Provence -s Corridor -s Windhorn -s Vland -s Meshan -s Lishun -s 'Trojan Reach' -s Reft -s Gushemege -s Dagudashaag -s Core -s "Riftspan Reaches" -s "Verge" -s "Ilelish" -s "Zarushagar" -s Massilia -s Antares -vvv
time target/release/traderust -d /var/tmp/traderoutes/ -o /var/tmp/traderust_output -s 'Spinward Marches' -s Deneb -s Gvurrdon -s Tuglikki -s Provence -s Corridor -s Windhorn -s Vland -s Meshan -s Lishun -s 'Trojan Reach' -s Reft -s Gushemege -s Dagudashaag -s Core -s "Riftspan Reaches" -s "Verge" -s "Ilelish" -s "Zarushagar" -s Massilia -s Antares -s "Empty Quarter" -vvv
time target/release/traderust -d /var/tmp/traderoutes/ -o /var/tmp/traderust_output -s 'Spinward Marches' -s Deneb -s Gvurrdon -s Tuglikki -s Provence -s Corridor -s Windhorn -s Vland -s Meshan -s Lishun -s 'Trojan Reach' -s Reft -s Gushemege -s Dagudashaag -s Core -s "Riftspan Reaches" -s "Verge" -s "Ilelish" -s "Zarushagar" -s Massilia -s Antares -s "Empty Quarter" -s Fornast -vvv
time target/release/traderust -d /var/tmp/traderoutes/ -o /var/tmp/traderust_output -s 'Spinward Marches' -s Deneb -s Gvurrdon -s Tuglikki -s Provence -s Corridor -s Windhorn -s Vland -s Meshan -s Lishun -s 'Trojan Reach' -s Reft -s Gushemege -s Dagudashaag -s Core -s "Riftspan Reaches" -s "Verge" -s "Ilelish" -s "Zarushagar" -s Massilia -s Antares -s "Empty Quarter" -s Fornast -s Ley -vvv
time target/release/traderust -d /var/tmp/traderoutes/ -o /var/tmp/traderust_output -s 'Spinward Marches' -s Deneb -s Gvurrdon -s Tuglikki -s Provence -s Corridor -s Windhorn -s Vland -s Meshan -s Lishun -s 'Trojan Reach' -s Reft -s Gushemege -s Dagudashaag -s Core -s "Riftspan Reaches" -s "Verge" -s "Ilelish" -s "Zarushagar" -s Massilia -s Antares -s "Empty Quarter" -s Fornast -s Ley -s Delphi -vvv
time target/release/traderust -d /var/tmp/traderoutes/ -o /var/tmp/traderust_output -s 'Spinward Marches' -s Deneb -s Gvurrdon -s Tuglikki -s Provence -s Corridor -s Windhorn -s Vland -s Meshan -s Lishun -s 'Trojan Reach' -s Reft -s Gushemege -s Dagudashaag -s Core -s "Riftspan Reaches" -s "Verge" -s "Ilelish" -s "Zarushagar" -s Massilia -s Antares -s "Empty Quarter" -s Fornast -s Ley -s Delphi -s "Glimmerdrift Reaches" -vvv
time target/release/traderust -d /var/tmp/traderoutes/ -o /var/tmp/traderust_output -s 'Spinward Marches' -s Deneb -s Gvurrdon -s Tuglikki -s Provence -s Corridor -s Windhorn -s Vland -s Meshan -s Lishun -s 'Trojan Reach' -s Reft -s Gushemege -s Dagudashaag -s Core -s "Riftspan Reaches" -s "Verge" -s "Ilelish" -s "Zarushagar" -s Massilia -s Antares -s "Empty Quarter" -s Fornast -s Ley -s Delphi -s "Glimmerdrift Reaches" -s Daibei -vvv
time target/release/traderust -d /var/tmp/traderoutes/ -o /var/tmp/traderust_output -s 'Spinward Marches' -s Deneb -s Gvurrdon -s Tuglikki -s Provence -s Corridor -s Windhorn -s Vland -s Meshan -s Lishun -s 'Trojan Reach' -s Reft -s Gushemege -s Dagudashaag -s Core -s "Riftspan Reaches" -s "Verge" -s "Ilelish" -s "Zarushagar" -s Massilia -s Antares -s "Empty Quarter" -s Fornast -s Ley -s Delphi -s "Glimmerdrift Reaches" -s Daibei -s Diaspora -vvv
time target/release/traderust -d /var/tmp/traderoutes/ -o /var/tmp/traderust_output -s 'Spinward Marches' -s Deneb -s Gvurrdon -s Tuglikki -s Provence -s Corridor -s Windhorn -s Vland -s Meshan -s Lishun -s 'Trojan Reach' -s Reft -s Gushemege -s Dagudashaag -s Core -s "Riftspan Reaches" -s "Verge" -s "Ilelish" -s "Zarushagar" -s Massilia -s Antares -s "Empty Quarter" -s Fornast -s Ley -s Delphi -s "Glimmerdrift Reaches" -s Daibei -s Diaspora -s "Old Expanses" -vvv
time target/release/traderust -d /var/tmp/traderoutes/ -o /var/tmp/traderust_output -s 'Spinward Marches' -s Deneb -s Gvurrdon -s Tuglikki -s Provence -s Corridor -s Windhorn -s Vland -s Meshan -s Lishun -s 'Trojan Reach' -s Reft -s Gushemege -s Dagudashaag -s Core -s "Riftspan Reaches" -s "Verge" -s "Ilelish" -s "Zarushagar" -s Massilia -s Antares -s "Empty Quarter" -s Fornast -s Ley -s Delphi -s "Glimmerdrift Reaches" -s Daibei -s Diaspora -s "Old Expanses" -s "Solomani Rim" -vvv
