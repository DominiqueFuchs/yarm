# Changelog

## [0.2.0](https://github.com/DominiqueFuchs/yarm/compare/v0.1.0...v0.2.0) (2026-01-31)


### Features

* add optional pos. name argument to profiles command, improve visual output separaton ([24d0acd](https://github.com/DominiqueFuchs/yarm/commit/24d0acdc333c4370f5dc7e9152ecf07fbb7b67d0))
* add repo to state data after clone/init op. within pool ([128b7cc](https://github.com/DominiqueFuchs/yarm/commit/128b7cca7cbee96b317d49c49f27bd04896a13d7))
* consolidate profile name and email output into single identity line ([21a51de](https://github.com/DominiqueFuchs/yarm/commit/21a51dece97aaddb84028a88b16fc57e8905c7a6))
* remove pos. arg from init command, make apply command accept repo name instead of path ([6446fa7](https://github.com/DominiqueFuchs/yarm/commit/6446fa7e1e902ccb98b8fb75bc9e3537b54b93b2))
* store and indicate yarm default profile attribute ([18e3ef0](https://github.com/DominiqueFuchs/yarm/commit/18e3ef0752a500ff40b58119eba4e67c0ed38bb9))
* use uppercase  pool option for all commands that support it, newly add pool option to apply command ([ee2b1f3](https://github.com/DominiqueFuchs/yarm/commit/ee2b1f3b5b290b5b91bf7e8485928b401194be23))


### Bug Fixes

* add missing blank line output formatting to ye(), preceding println to commands ([24a6a12](https://github.com/DominiqueFuchs/yarm/commit/24a6a121e64ddcebe803d3c62caf2236386ee0f7))
* add missing ye() navigation confirmation cli output ([618a88b](https://github.com/DominiqueFuchs/yarm/commit/618a88bd184986a39e982759434206187eb7663f))
* change profile list default name for generic .gitconfig to global to prevent confusion with yarm default ([f7e79f4](https://github.com/DominiqueFuchs/yarm/commit/f7e79f4ba355d90f463ed0daa79785a1458f2f1b))
* correct term output format for profile edit history, remove spurious printlines ([44f9ffb](https://github.com/DominiqueFuchs/yarm/commit/44f9ffbc44e858e0564e87ef2cd7b9f2a04e6237))
* don't dim() red field value colors to improve readability on common dark-not-black modes ([5b0869a](https://github.com/DominiqueFuchs/yarm/commit/5b0869af6c8a3ba7f8bd9f4eda91fcba4158c73a))
* exit menu on successful profile operation instead of restart ([3aff4d3](https://github.com/DominiqueFuchs/yarm/commit/3aff4d3e53cfc09171b87fc5a5a63a2244601ff0))
* show correct signing format change for transitions from or to openpgp ([66a1669](https://github.com/DominiqueFuchs/yarm/commit/66a1669fbc793a345c5354e5b96520ad827d24ed))
* show profile config values whenever set, improve readability ([9e38a7f](https://github.com/DominiqueFuchs/yarm/commit/9e38a7f13571642d7cded80091ba508faab54a5a))

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
