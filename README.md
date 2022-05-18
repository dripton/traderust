This program calculates galactic trade routes using the rules in GURPS
Traveller: Far Trader.

https://github.com/makhidkarun/traveller_pyroute already existed, but I
couldn't get it to work, so I wrote my own Python version
https://github.com/dripton/traderoutes

But it was too slow, so I ported it to Rust, and now it's much faster.
(Roughly 350 times as fast as my Python version for large maps.)


Runtime dependencies:

* http://travellermap.com if you haven't already downloaded data locally

Development dependencies:

* rustc (I used 1.60 stable)
* cargo
* git and GitHub for version control

You can look in Cargo.toml for crates it depends on.  Some of the key
dependencies are:
* cairo-rs (drawing the map and producing the PDFs)
* rayon (fast and fairly painless parallelism)
* rstest (unit test fixtures)
* reqwest (http client)
* elementtree (XML parsing)
* clap (command-line argument parsing)
* lazy_static (ability to make a static hashmap)
* ndarray (NumPy-style 2D arrays)
* rand (random numbers)

Thanks to everyone involved with all of the above.

Building:

* Install Rust and Cargo (https://www.rust-lang.org/learn/get-started)
* "cargo test" to run unit tests
* "cargo build" to build a dev version
* "cargo build -r" to build a release version (faster)

Running:

* "cargo run -r -- -h" for help
* "cargo run -r -- -s 'Spinward Marches' -o "/tmp"
   should download the Spinward Marches subsector data from travellermap.com
   and then generate "/tmp/Spinward Marches.pdf" which you can view with your
   favorite PDF viewer.
