# Changelog

## [0.4.0](https://github.com/JorisJonkers-dev/nvmod-rs/compare/v0.3.0...v0.4.0) (2026-11-14)


### Features

* **backup:** nvmod backup create, list, restore and prune with automatic snapshots ([#44](https://github.com/JorisJonkers-dev/nvmod-rs/issues/44)) ([bd7a6f6](https://github.com/JorisJonkers-dev/nvmod-rs/commit/bd7a6f6632564ef7d591580e976daaf79130c96c))
* **conflicts:** add nvmod conflicts and nvmod tree ([#42](https://github.com/JorisJonkers-dev/nvmod-rs/issues/42)) ([71a34de](https://github.com/JorisJonkers-dev/nvmod-rs/commit/71a34de5737774208cacbd97ad85797b58be2999))
* **formats:** meta.ini editor, version type and meta.ini on install ([#41](https://github.com/JorisJonkers-dev/nvmod-rs/issues/41)) ([657f085](https://github.com/JorisJonkers-dev/nvmod-rs/commit/657f085f57d58fd8598d26e6b843634e04f43f58))
* **lod:** add nvmod lod plan through the pinned lodwright-rs ([#30](https://github.com/JorisJonkers-dev/nvmod-rs/issues/30)) ([#39](https://github.com/JorisJonkers-dev/nvmod-rs/issues/39)) ([ac0b7e4](https://github.com/JorisJonkers-dev/nvmod-rs/commit/ac0b7e4701ccb8842f63e203813e4c7b42ba876e))
* **plugins:** add nvmod plugins commands and sort --fetch-masterlist ([#49](https://github.com/JorisJonkers-dev/nvmod-rs/issues/49)) ([b777724](https://github.com/JorisJonkers-dev/nvmod-rs/commit/b777724f3a9fbb87b8bd1c958b87151b8d39a9db))
* **profile:** materialise a profile in the prefix (INIs, tweak merge, local saves, plugin lists, archive invalidation) ([#54](https://github.com/JorisJonkers-dev/nvmod-rs/issues/54)) ([867031e](https://github.com/JorisJonkers-dev/nvmod-rs/commit/867031e8d440f972304825923b88317e4c9885a4))
* **runlog:** tracing in the library crates and a local run log for every command ([#48](https://github.com/JorisJonkers-dev/nvmod-rs/issues/48)) ([d4ac133](https://github.com/JorisJonkers-dev/nvmod-rs/commit/d4ac1332fae52a71c5443518800ae6ff5507d60c))
* **saves:** nvmod saves list, backup, restore, transfer, check and checksum ([#55](https://github.com/JorisJonkers-dev/nvmod-rs/issues/55)) ([5dca00a](https://github.com/JorisJonkers-dev/nvmod-rs/commit/5dca00ae33e667474cfa5ebfe2511a608266c132))
* **setup:** keep the imported MO2 load order and make the sort opt-in ([#57](https://github.com/JorisJonkers-dev/nvmod-rs/issues/57)) ([8cc8354](https://github.com/JorisJonkers-dev/nvmod-rs/commit/8cc83547eeacc9140fc365b8dcfe93a8a2acef0e))
* **tools:** negotiate contracts with the tool, clamped to the lock's version ([#50](https://github.com/JorisJonkers-dev/nvmod-rs/issues/50)) ([ee0cad6](https://github.com/JorisJonkers-dev/nvmod-rs/commit/ee0cad6c3ebbb851190faa8a08a08b0ed1fa69a2))
* **usage:** add nvmod nvse-usage, an opcode-only scan of the active plugins' compiled scripts ([#46](https://github.com/JorisJonkers-dev/nvmod-rs/issues/46)) ([33296d8](https://github.com/JorisJonkers-dev/nvmod-rs/commit/33296d8e3b866f30ef3d8d8ed856d820f02cd1da))


### Bug Fixes

* **ci:** build release assets from the workflow's own packaging scripts ([#53](https://github.com/JorisJonkers-dev/nvmod-rs/issues/53)) ([ca869c4](https://github.com/JorisJonkers-dev/nvmod-rs/commit/ca869c420839c27080852794f54c13c126ba299d))
* **cli:** give nvse-usage opcode blocks only the ranges plugins register ([#60](https://github.com/JorisJonkers-dev/nvmod-rs/issues/60)) ([6e19e4b](https://github.com/JorisJonkers-dev/nvmod-rs/commit/6e19e4b33a87d20c26884eed1716aae5dbcb886c))
* **release:** do not parse an empty pull request output in the lock refresh step ([#52](https://github.com/JorisJonkers-dev/nvmod-rs/issues/52)) ([b064013](https://github.com/JorisJonkers-dev/nvmod-rs/commit/b0640136891832994b1126925087f35d5bed91b3))
* **tests:** stop running freshly written executables into ETXTBSY ([#59](https://github.com/JorisJonkers-dev/nvmod-rs/issues/59)) ([5ee5c3c](https://github.com/JorisJonkers-dev/nvmod-rs/commit/5ee5c3cd575e9abb480cc83495676ab886bbe3df))

## [0.3.0](https://github.com/JorisJonkers-dev/nvmod-rs/compare/v0.2.0...v0.3.0) (2026-11-14)


### Features

* **cli:** roll back on SIGTERM and SIGINT, and make journal ids collision-free ([#36](https://github.com/JorisJonkers-dev/nvmod-rs/issues/36)) ([56075a5](https://github.com/JorisJonkers-dev/nvmod-rs/commit/56075a5b998a4e467918e2ef06f53fbeb637006f))
* **tools:** take loot-rs v0.2.0 and state the contract versions nvmod accepts ([#38](https://github.com/JorisJonkers-dev/nvmod-rs/issues/38)) ([235209a](https://github.com/JorisJonkers-dev/nvmod-rs/commit/235209a2a75db198ad5d3403f5c96d970c2ae92b))

## [0.2.0](https://github.com/JorisJonkers-dev/nvmod-rs/compare/v0.1.0...v0.2.0) (2026-11-14)


### Features

* add the Instance skeleton, the Doctor on a base copy and the exit code table ([#6](https://github.com/JorisJonkers-dev/nvmod-rs/issues/6)) ([35843c3](https://github.com/JorisJonkers-dev/nvmod-rs/commit/35843c3ba6a28ff6cdc428495a870126a7ec092c))
* add versioned tool contracts and the one subprocess runner ([#17](https://github.com/JorisJonkers-dev/nvmod-rs/issues/17)) ([e8a38b9](https://github.com/JorisJonkers-dev/nvmod-rs/commit/e8a38b9ae71ff743aef180180eeab950db5a9c74))
* build nv-rs from source or fetch the verified release with nvmod install-nv-rs ([#12](https://github.com/JorisJonkers-dev/nvmod-rs/issues/12)) ([7d84c4c](https://github.com/JorisJonkers-dev/nvmod-rs/commit/7d84c4c9df436118e5cd41b900f674f2b44a9c83))
* build the Staging tree with nvmod stage ([#9](https://github.com/JorisJonkers-dev/nvmod-rs/issues/9)) ([1393dc6](https://github.com/JorisJonkers-dev/nvmod-rs/commit/1393dc6c0e0eb0f74eb74fd55cefffcab38bc985))
* **catalog:** add entries for the five mods the Doctor run found missing ([#30](https://github.com/JorisJonkers-dev/nvmod-rs/issues/30)) ([a5af6a3](https://github.com/JorisJonkers-dev/nvmod-rs/commit/a5af6a312232dd89c7cb416aa40e64be1ded6e87))
* **cli:** add nvmod mods list, enable, disable and reorder ([#33](https://github.com/JorisJonkers-dev/nvmod-rs/issues/33)) ([34fd85a](https://github.com/JorisJonkers-dev/nvmod-rs/commit/34fd85ac4ff06b2e57f9808bc70efc9660681ea4))
* compute the Load order with loot-rs and encode it in plugin file times with nvmod sort ([#15](https://github.com/JorisJonkers-dev/nvmod-rs/issues/15)) ([ef0a2e0](https://github.com/JorisJonkers-dev/nvmod-rs/commit/ef0a2e032f2c6679c21d0129b49a5e63b6d14fed))
* confirm an install with nvmod verify and print the launch plan ([#23](https://github.com/JorisJonkers-dev/nvmod-rs/issues/23)) ([2821193](https://github.com/JorisJonkers-dev/nvmod-rs/commit/282119320ab220f37ec84bf0741ae98e48f24667))
* give a command line that is not understood its own exit code, 5 ([#13](https://github.com/JorisJonkers-dev/nvmod-rs/issues/13)) ([732f59d](https://github.com/JorisJonkers-dev/nvmod-rs/commit/732f59d492ba029341e8d43b389c65759a617069))
* import one MO2 profile into an nvmod Instance with nvmod import-mo2 ([#11](https://github.com/JorisJonkers-dev/nvmod-rs/issues/11)) ([868fdc3](https://github.com/JorisJonkers-dev/nvmod-rs/commit/868fdc3186a547c501fa2d1d4d0c6a02a0c840a8))
* install a Mod from an archive with FOMOD inference, and roll it back ([#14](https://github.com/JorisJonkers-dev/nvmod-rs/issues/14)) ([f604703](https://github.com/JorisJonkers-dev/nvmod-rs/commit/f6047037048385f87f2d6fe40b081623215cef76))
* install Tale of Two Wastelands through the pinned external installer with nvmod ttw ([#20](https://github.com/JorisJonkers-dev/nvmod-rs/issues/20)) ([fca7e12](https://github.com/JorisJonkers-dev/nvmod-rs/commit/fca7e12cc3b4ca40741bc35026d645914cf8d9de))
* model what is built in per target and report it per target in the Doctor ([#21](https://github.com/JorisJonkers-dev/nvmod-rs/issues/21)) ([e3d6263](https://github.com/JorisJonkers-dev/nvmod-rs/commit/e3d62635310224a8f7c8301c217c723017ee8923))
* own the Instance and Profile model in nvmod-core with a generated schema and a tested migration ([#16](https://github.com/JorisJonkers-dev/nvmod-rs/issues/16)) ([aad67a1](https://github.com/JorisJonkers-dev/nvmod-rs/commit/aad67a13eaaeec3ff2fb1be616f11dea2360c8cc))
* pin external tools in tools.lock.toml and fetch them with verification ([#18](https://github.com/JorisJonkers-dev/nvmod-rs/issues/18)) ([86f6ac3](https://github.com/JorisJonkers-dev/nvmod-rs/commit/86f6ac3f7b5efadeee925ea9a223c181578b7557))
* run nv-viewer --check instead of a play-patch step in nvmod install-nv-rs ([#19](https://github.com/JorisJonkers-dev/nvmod-rs/issues/19)) ([90d3112](https://github.com/JorisJonkers-dev/nvmod-rs/commit/90d3112c167632eeb23270913dc63013a3f534ea))
* **setup:** make nvmod setup resumable ([#25](https://github.com/JorisJonkers-dev/nvmod-rs/issues/25)) ([203c589](https://github.com/JorisJonkers-dev/nvmod-rs/commit/203c5896d3535443a172d4e94057f14e92c7f4e6))
* take a base copy to a Doctor-clean Instance in one run with nvmod setup ([#22](https://github.com/JorisJonkers-dev/nvmod-rs/issues/22)) ([bdeb3e8](https://github.com/JorisJonkers-dev/nvmod-rs/commit/bdeb3e88e41a08f48e5b5fa12317efa48a321eb5))
* **testkit:** add nvmod-fixtures, a binary that writes the synthetic setup fixtures to a folder ([#26](https://github.com/JorisJonkers-dev/nvmod-rs/issues/26)) ([42b80f0](https://github.com/JorisJonkers-dev/nvmod-rs/commit/42b80f037a0dafe1631541f4519c6aa30bea703b))
* **ui-model:** add nvmod-ui-model, the toolkit-free view-model of the GUI ([#31](https://github.com/JorisJonkers-dev/nvmod-rs/issues/31)) ([6637cda](https://github.com/JorisJonkers-dev/nvmod-rs/commit/6637cda1dc6453ec682f3955caf6f927c20a4c29))
* **ui-service:** add nvmod-ui-service, the service seam of the GUI ([#32](https://github.com/JorisJonkers-dev/nvmod-rs/issues/32)) ([e385a12](https://github.com/JorisJonkers-dev/nvmod-rs/commit/e385a1281615135bd4dfd0f7caf598116fd7672b))
* **ui:** apply Mods edits, typed setup request and job cancel ([#35](https://github.com/JorisJonkers-dev/nvmod-rs/issues/35)) ([57c5f36](https://github.com/JorisJonkers-dev/nvmod-rs/commit/57c5f36f7d3094151ff51c225efbd3063b62657d))
* **ui:** typed calls for sort, verify, target, tools and rollback, job streaming, Doctor and Mods state ([#34](https://github.com/JorisJonkers-dev/nvmod-rs/issues/34)) ([b97191e](https://github.com/JorisJonkers-dev/nvmod-rs/commit/b97191ecf2c9d6b5bc9761e6b2fa403422a43b2f))


### Bug Fixes

* **contracts:** vendor the nv-rs fork's real release-manifest schema ([#24](https://github.com/JorisJonkers-dev/nvmod-rs/issues/24)) ([ccff82b](https://github.com/JorisJonkers-dev/nvmod-rs/commit/ccff82b491d85e4c8bb6609e343d360d77096f21))
* **release:** bump the lock entries of every workspace crate ([#27](https://github.com/JorisJonkers-dev/nvmod-rs/issues/27)) ([e9129a8](https://github.com/JorisJonkers-dev/nvmod-rs/commit/e9129a8c17e95a6aefb8b505217002dde2cdc6d1))
* **release:** refresh the lock entries on the release branch ([#28](https://github.com/JorisJonkers-dev/nvmod-rs/issues/28)) ([79fc286](https://github.com/JorisJonkers-dev/nvmod-rs/commit/79fc286491b6be48c38e71637f209427fbf360fd))

## 0.1.0 (2026-11-14)


### Features

* inspect PE files safely and add the pe command ([#2](https://github.com/JorisJonkers-dev/nvmod-rs/issues/2)) ([0e7de35](https://github.com/JorisJonkers-dev/nvmod-rs/commit/0e7de35825e1c79d53cadb6d5bf580d07c3234ef))
* read an MO2 instance and check NVSE plugins with the doctor ([#4](https://github.com/JorisJonkers-dev/nvmod-rs/issues/4)) ([51c0183](https://github.com/JorisJonkers-dev/nvmod-rs/commit/51c01830f86dbf8832cf5a27632cd635e583a7e6))
