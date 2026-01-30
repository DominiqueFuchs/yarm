# Changelog

## 0.1.0 (2026-01-30)


### Features

* add --pool/-p option to find and enter configured pool locations ([ad85d73](https://github.com/DominiqueFuchs/yarm/commit/ad85d738bce37a1dd2a1317898a05ecc4ea2f9fd))
* add apply command for existing repositories ([5e4d44e](https://github.com/DominiqueFuchs/yarm/commit/5e4d44ece2c567f2b840a459aef9ba4958123c9e))
* add auto_rescan option, triggering scan operations after application updates with state version change ([d25bfff](https://github.com/DominiqueFuchs/yarm/commit/d25bfffa0637bcb40feb0d5e63fb2e79053b1e86))
* add configuration file incl. [profiles] option section ([0893aaa](https://github.com/DominiqueFuchs/yarm/commit/0893aaa687b3e8ee5e1b98e4d4157260f2084b67))
* add exclude option to [repositories] section, allow exclude patterns ([3e1310e](https://github.com/DominiqueFuchs/yarm/commit/3e1310e92c97427ff3bc60434448c431be7384e0))
* add find command and ye() completions shell function ([56491eb](https://github.com/DominiqueFuchs/yarm/commit/56491eb653c2380121d218751110d462786e9340))
* add IncludeIf awareness for profile selection ([bd50997](https://github.com/DominiqueFuchs/yarm/commit/bd50997af865da53ed1393bfbd1a0ec3dbeebbee))
* add new configuration options for profiles ([05822f2](https://github.com/DominiqueFuchs/yarm/commit/05822f2b646a0bf3a7dc7e104ecea71d7081669c))
* add scan and status command, implement persistent state data ([3488c17](https://github.com/DominiqueFuchs/yarm/commit/3488c17fbcd064b519851bfb19be479f10d04df5))
* add stat command ([27e8f25](https://github.com/DominiqueFuchs/yarm/commit/27e8f2530f2d01aa8623b20573d9f08be32f9d86))
* implement autocomplete functionality to work with pool and repo names ([cda2249](https://github.com/DominiqueFuchs/yarm/commit/cda22499a72c2a32655e71fb490ac5f8f9262cf0))


### Bug Fixes

* cast directly as f64, prevent clamping of huge values in format_count ([d5dfee9](https://github.com/DominiqueFuchs/yarm/commit/d5dfee91321596538878b6ef237264e24e0fab15))
* exit cleanly when leaving top-level menu via ESC ([d6cc2b8](https://github.com/DominiqueFuchs/yarm/commit/d6cc2b8aadc3dbd81354d8df4cf5fde9770287e0))
