# Changelog

## [0.8.1](https://github.com/JorisJonkers-dev/nvmod-rs/compare/v0.8.0...v0.8.1) (2026-11-18)


### Bug Fixes

* **deps:** use caret requirements so the Components link together ([#172](https://github.com/JorisJonkers-dev/nvmod-rs/issues/172)) ([44f2c7b](https://github.com/JorisJonkers-dev/nvmod-rs/commit/44f2c7b7fe97e57c39c95116757d0bda1daeb379))

## [0.8.0](https://github.com/JorisJonkers-dev/nvmod-rs/compare/v0.7.0...v0.8.0) (2026-11-18)


### Features

* **api:** add the install-nv-rs job ([#163](https://github.com/JorisJonkers-dev/nvmod-rs/issues/163)) ([8c58c83](https://github.com/JorisJonkers-dev/nvmod-rs/commit/8c58c83fb94671cc03dbdabfa8317a6b298f28c4))
* **api:** add the ttw job ([#162](https://github.com/JorisJonkers-dev/nvmod-rs/issues/162)) ([a021304](https://github.com/JorisJonkers-dev/nvmod-rs/commit/a021304d28149d9e8aa74964c8801ccdc9febb34))
* **api:** import-mo2 job in the library API ([#159](https://github.com/JorisJonkers-dev/nvmod-rs/issues/159)) ([ca3a831](https://github.com/JorisJonkers-dev/nvmod-rs/commit/ca3a831928449f66d9e7b75a9764599a46adc10e))
* **api:** profile create job in the library API ([#154](https://github.com/JorisJonkers-dev/nvmod-rs/issues/154)) ([459fb90](https://github.com/JorisJonkers-dev/nvmod-rs/commit/459fb9050e931a6c13d79c6fde30131c01b07808))
* **api:** sort apply and stamp job in the library API ([#161](https://github.com/JorisJonkers-dev/nvmod-rs/issues/161)) ([4c918df](https://github.com/JorisJonkers-dev/nvmod-rs/commit/4c918df34f095881ce997b499c7ab89f2b74d681))
* **api:** stage job in the library API ([#156](https://github.com/JorisJonkers-dev/nvmod-rs/issues/156)) ([bdb5b76](https://github.com/JorisJonkers-dev/nvmod-rs/commit/bdb5b76a8217bd5ac2aa1f6dbd79b57d0e95860d))
* **api:** target job in the library API ([#157](https://github.com/JorisJonkers-dev/nvmod-rs/issues/157)) ([db443d3](https://github.com/JorisJonkers-dev/nvmod-rs/commit/db443d3036ebd5dad38b0a253b4e21a59d05a735))
* **api:** verify job in the library API ([#160](https://github.com/JorisJonkers-dev/nvmod-rs/issues/160)) ([4cb98b7](https://github.com/JorisJonkers-dev/nvmod-rs/commit/4cb98b7f804eb21d198cf22aa84af88b122059c0))

## [0.7.0](https://github.com/JorisJonkers-dev/nvmod-rs/compare/v0.6.2...v0.7.0) (2026-11-17)


### Features

* **api:** add nvmod-api, a library API called in process ([#118](https://github.com/JorisJonkers-dev/nvmod-rs/issues/118)) ([4c83d52](https://github.com/JorisJonkers-dev/nvmod-rs/commit/4c83d52df294ea50a472abed2d2de39c26789591))
* **api:** backup jobs in the library API ([#127](https://github.com/JorisJonkers-dev/nvmod-rs/issues/127)) ([2bde9c1](https://github.com/JorisJonkers-dev/nvmod-rs/commit/2bde9c1b558e9e6a7ae9f1cb4b5df32a2538c0aa))
* **api:** init job in the library API ([#135](https://github.com/JorisJonkers-dev/nvmod-rs/issues/135)) ([d6315b6](https://github.com/JorisJonkers-dev/nvmod-rs/commit/d6315b6ec9213d3641f08eee8efcbcb93b646264))
* **api:** plugin jobs in the library API ([#128](https://github.com/JorisJonkers-dev/nvmod-rs/issues/128)) ([1ff9c93](https://github.com/JorisJonkers-dev/nvmod-rs/commit/1ff9c93ba71030ee4b19dc13f566390a2b54d27e))
* **api:** rollback and install jobs in the library API ([#130](https://github.com/JorisJonkers-dev/nvmod-rs/issues/130)) ([6d59ae1](https://github.com/JorisJonkers-dev/nvmod-rs/commit/6d59ae1cd605a21110360d0324a2cdb632683fc7))
* **api:** saves backup, restore and transfer jobs ([#133](https://github.com/JorisJonkers-dev/nvmod-rs/issues/133)) ([4c1b922](https://github.com/JorisJonkers-dev/nvmod-rs/commit/4c1b922ed367168506849603f4cfa656bd8da635))
* **api:** saves check and checksum jobs ([#132](https://github.com/JorisJonkers-dev/nvmod-rs/issues/132)) ([71d317b](https://github.com/JorisJonkers-dev/nvmod-rs/commit/71d317b01c8bddec62dd718ff56f125380f90922))
* **api:** the install job takes answers and limits ([#136](https://github.com/JorisJonkers-dev/nvmod-rs/issues/136)) ([e89915f](https://github.com/JorisJonkers-dev/nvmod-rs/commit/e89915f68baa0917bd08be2b80af0cccedae0020))

## [0.6.2](https://github.com/JorisJonkers-dev/nvmod-rs/compare/v0.6.1...v0.6.2) (2026-11-17)


### Bug Fixes

* **formats:** keep the paths of a native Windows MO2 as host paths ([#104](https://github.com/JorisJonkers-dev/nvmod-rs/issues/104)) ([0a2fa75](https://github.com/JorisJonkers-dev/nvmod-rs/commit/0a2fa7585791b3b57bfbc74f061f1e054eb49b62))
* **install:** pass the Windows build environment to cargo ([#103](https://github.com/JorisJonkers-dev/nvmod-rs/issues/103)) ([d10cc4f](https://github.com/JorisJonkers-dev/nvmod-rs/commit/d10cc4fb02c064cc9a1ab775d1e4b141479e88c0))
* **loadorder:** give hard-linked plugins a copy of their own on Windows ([#106](https://github.com/JorisJonkers-dev/nvmod-rs/issues/106)) ([77b0fd1](https://github.com/JorisJonkers-dev/nvmod-rs/commit/77b0fd162e82258261af8b7c6c4bb2fe3f7e76bb))
* **profile:** remove the save folder link the Windows way ([#105](https://github.com/JorisJonkers-dev/nvmod-rs/issues/105)) ([d01f2a9](https://github.com/JorisJonkers-dev/nvmod-rs/commit/d01f2a9325cec9c110249db46675c2007439d05a))
* **runlog:** mask UNC and verbatim Windows paths in the run log ([#108](https://github.com/JorisJonkers-dev/nvmod-rs/issues/108)) ([b94df65](https://github.com/JorisJonkers-dev/nvmod-rs/commit/b94df65a5927d52350e35f45ad1a4d563a60444a))
* **run:** tell on macOS when the holder of the Instance lock is gone, and gate on Windows and macOS ([#110](https://github.com/JorisJonkers-dev/nvmod-rs/issues/110)) ([e9b7592](https://github.com/JorisJonkers-dev/nvmod-rs/commit/e9b7592a4ce75093fd3c7195b2df57002f44d238))
* write plain Windows paths instead of verbatim ones ([#107](https://github.com/JorisJonkers-dev/nvmod-rs/issues/107)) ([db980d3](https://github.com/JorisJonkers-dev/nvmod-rs/commit/db980d317f78d92ca0aaa01afb2cf4622341b30f))

## [0.6.1](https://github.com/JorisJonkers-dev/nvmod-rs/compare/v0.6.0...v0.6.1) (2026-11-17)


### Bug Fixes

* **deps:** pin dependencies ([#99](https://github.com/JorisJonkers-dev/nvmod-rs/issues/99)) ([c0b384f](https://github.com/JorisJonkers-dev/nvmod-rs/commit/c0b384f9acfeb94d6e82114cdfbc2ec46e38c445))

## [0.6.0](https://github.com/JorisJonkers-dev/nvmod-rs/compare/v0.5.0...v0.6.0) (2026-11-16)


### Features

* **archives:** BSA directory reader, archive conflicts and the archive list audit ([#64](https://github.com/JorisJonkers-dev/nvmod-rs/issues/64)) ([6d7ef69](https://github.com/JorisJonkers-dev/nvmod-rs/commit/6d7ef6920bc43e8c16ff7e42725838dd6f733187))
* **doctor:** per-category nv-rs readiness from the catalogue, capabilities.json and the load order ([#68](https://github.com/JorisJonkers-dev/nvmod-rs/issues/68)) ([e0e3b16](https://github.com/JorisJonkers-dev/nvmod-rs/commit/e0e3b160c8097015d9b160b72bb6385524cdc5a6))
* **run:** run a modded profile natively on nv-rs ([#66](https://github.com/JorisJonkers-dev/nvmod-rs/issues/66)) ([7b3a07d](https://github.com/JorisJonkers-dev/nvmod-rs/commit/7b3a07d0e5fb29f2330f8704947ae0dc6ae7aefa))
* **tools:** take loot-rs v0.5.0 ([#67](https://github.com/JorisJonkers-dev/nvmod-rs/issues/67)) ([3d184f6](https://github.com/JorisJonkers-dev/nvmod-rs/commit/3d184f69d9bfd336f106a5f1c985d819f01ff130))

## [0.5.0](https://github.com/JorisJonkers-dev/nvmod-rs/compare/v0.4.0...v0.5.0) (2026-11-15)


### Features

* **gui:** add the nested gui/ workspace with a GPUI setup window, an egui canary and a GUI workflow ([#62](https://github.com/JorisJonkers-dev/nvmod-rs/issues/62)) ([0745340](https://github.com/JorisJonkers-dev/nvmod-rs/commit/07453402a0ab296bf2ccaba9c2614983da72c39a))
* **run:** executables registry, instance lock and nvmod run through the staged view ([#51](https://github.com/JorisJonkers-dev/nvmod-rs/issues/51)) ([13356df](https://github.com/JorisJonkers-dev/nvmod-rs/commit/13356dfb51b43e49ea29beccdbf69b603a09fc1e))

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
