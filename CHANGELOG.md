# Changelog

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
