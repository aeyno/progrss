# Progrss

A clone of [progress](https://github.com/Xfennec/progress) to view the progress of your running coreutils command (cv, mv, dd, cat, ...).

⚠️Warning : this project is under heavy development ⚠️

This project doesn't aim to be a drop-in replacement of progress but its goal is to provides a least all its functionnalities and possibly some command specific outputs.


## Build from source 🔨

Clone this repo then

```bash
cargo run
```


## RoadMap 📜

- [x] Basic informations about running commands (pid, open filedescriptors, percentage)
- [x] Target specific PIDs (`-p --pid`)
- [x] Target specific command (`-c --command`)
- [x] Throughput estimation
- [ ] Monitor mode
  - [ ] Monitor until running command terminates (`-m --monitor`)
  - [ ] Monitor continuously (`-M --monitor-continuously`)
- [x] Add additionnal command to watch (`-a --additional-command`)
- [ ] Use specific file mode to estimate progress (`-o --open-mode`)
- [ ] Command specific output (change output depending on the command monitored)
- [ ] Progress bar
- [ ] OS compatibility :
  - [x] Linux
  - [ ] BSD
  - [ ] MacOS
