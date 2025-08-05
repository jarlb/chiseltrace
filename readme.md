# ChiselTrace: Automatic Signal Dependency Tracing for Chisel

This repository contains the ChiselTrace project. ChiselTrace is a source-level debugging tool for the Chisel hardware construction language. By automatically analysing and visualising simulation-time data- and control-flow dependencies between statements in the Chisel source code, ChiselTrace aims to reduce the amount of time spent tracing back faults to the root cause in the waveform viewer, thereby bringing the Chisel debugging ecosystem closer to that of classical HDLs.

ChiselTrace builds on the [Tywaves](https://github.com/rameloni/tywaves-chisel) project, a typed waveform viewer for Chisel. ChiselTrace implements the following stages:

- A Chisel extension that extracts a program dependence graph and a control flow graph from a FIRRTL circuit, while inserting instrumentation probes.

- A Rust library (chiseltrace-rs) that takes the produced graphs and synthesises this information, along with a VCD file and CIRCT debug information, into a dynamic program dependence graph (only dependencies that occurred in the simulation) that is annotated with typed Tywaves simulation data.

- A graph viewer front-end that enables interactive dependency exploration

- An extension to ChiselSim to automatically launch ChiselTrace on failed assertions.

![An example of ChiselTrace](/img/example1.png)

## Installation

### Dependencies

### Installing ChiselTrace Components

## Getting Started

## Features

## Case-study

## Future Work

## License

This work is licensed under the [Apache Licence 2.0](https://www.apache.org/licenses/LICENSE-2.0).
