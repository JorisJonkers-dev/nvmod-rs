# Feature coverage

Generated on main by tally from `catalogue/` (docs/TESTING.md, "Feature catalogue"). Do not edit.

| | Implemented | Tagged | Passing | Covered | Missing | Gated | n/a, deferred |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| **Total** | 292/292 | 291 | 291 | 258 | 0 | 1 | 4 |
| **Library and command line** | 21/21 | 21 | 21 | 19 | 0 | 0 | 1 |
| The nvmod command (`api.cli`) | 11/11 | 11 | 11 | 9 | 0 | 0 | 1 |
| The nvmod-api jobs (`api.jobs`) | 10/10 | 10 | 10 | 10 | 0 | 0 | 0 |
| **Doctor and verification** | 41/41 | 41 | 41 | 41 | 0 | 0 | 0 |
| Checking a game copy (`doctor.base`) | 6/6 | 6 | 6 | 6 | 0 | 0 | 0 |
| Checks of an Instance (`doctor.checks`) | 16/16 | 16 | 16 | 16 | 0 | 0 | 0 |
| NVSE plugins and the catalogue (`doctor.nvse`) | 14/14 | 14 | 14 | 14 | 0 | 0 | 0 |
| verify (`doctor.verify`) | 5/5 | 5 | 5 | 5 | 0 | 0 | 0 |
| **File readers** | 23/23 | 23 | 23 | 22 | 0 | 0 | 1 |
| BSA archives (`formats.archives`) | 5/5 | 5 | 5 | 5 | 0 | 0 | 1 |
| Text, ini and binary file readers (`formats.files`) | 11/11 | 11 | 11 | 10 | 0 | 0 | 0 |
| PE files (`formats.pe`) | 7/7 | 7 | 7 | 7 | 0 | 0 | 0 |
| **Instance and Profiles** | 20/20 | 20 | 20 | 20 | 0 | 0 | 0 |
| Creating an Instance (`instance.init`) | 6/6 | 6 | 6 | 6 | 0 | 0 | 0 |
| Instance file and schemas (`instance.model`) | 5/5 | 5 | 5 | 5 | 0 | 0 | 0 |
| Profiles (`instance.profiles`) | 9/9 | 9 | 9 | 9 | 0 | 0 | 0 |
| **Mods** | 55/55 | 55 | 55 | 55 | 0 | 0 | 2 |
| The downloads library (`mods.downloads`) | 6/6 | 6 | 6 | 6 | 0 | 0 | 2 |
| FOMOD installers (`mods.fomod`) | 11/11 | 11 | 11 | 11 | 0 | 0 | 0 |
| Installing a mod (`mods.install`) | 14/14 | 14 | 14 | 14 | 0 | 0 | 0 |
| The life of a Mod (`mods.lifecycle`) | 18/18 | 18 | 18 | 18 | 0 | 0 | 0 |
| The Overwrite folder (`mods.overwrite`) | 6/6 | 6 | 6 | 6 | 0 | 0 | 0 |
| **Plugins and load order** | 25/25 | 25 | 25 | 25 | 0 | 0 | 0 |
| Plugins (`plugins.order`) | 12/12 | 12 | 12 | 12 | 0 | 0 | 0 |
| Sorting the load order (`plugins.sort`) | 13/13 | 13 | 13 | 13 | 0 | 0 | 0 |
| **Running programs** | 24/24 | 23 | 23 | 12 | 0 | 1 | 0 |
| Starting the game and other programs (`run.launch`) | 11/11 | 10 | 10 | 1 | 0 | 1 | 0 |
| Placing a Profile in the game's prefix (`run.prefix`) | 10/10 | 10 | 10 | 10 | 0 | 0 | 0 |
| Programs registry (`run.registry`) | 3/3 | 3 | 3 | 1 | 0 | 0 | 0 |
| **Backups, saves and import** | 27/27 | 27 | 27 | 23 | 0 | 0 | 0 |
| Profile backups (`safety.backup`) | 8/8 | 8 | 8 | 8 | 0 | 0 | 0 |
| Importing from Mod Organizer 2 (`safety.import`) | 7/7 | 7 | 7 | 6 | 0 | 0 | 0 |
| Saves (`safety.saves`) | 7/7 | 7 | 7 | 7 | 0 | 0 | 0 |
| Undo and interruption (`safety.undo`) | 5/5 | 5 | 5 | 2 | 0 | 0 | 0 |
| **Staging tree and conflicts** | 30/30 | 30 | 30 | 29 | 0 | 0 | 0 |
| Conflicts (`staging.conflicts`) | 10/10 | 10 | 10 | 10 | 0 | 0 | 0 |
| Sweeping what a program wrote (`staging.sweep`) | 7/7 | 7 | 7 | 7 | 0 | 0 | 0 |
| The Staging tree (`staging.tree`) | 13/13 | 13 | 13 | 12 | 0 | 0 | 0 |
| **External tools and contracts** | 26/26 | 26 | 26 | 12 | 0 | 0 | 0 |
| Contracts with the other Components (`tools.contracts`) | 10/10 | 10 | 10 | 9 | 0 | 0 | 0 |
| Pinned tools (`tools.pins`) | 11/11 | 11 | 11 | 2 | 0 | 0 | 0 |
| Running external tools safely (`tools.runner`) | 5/5 | 5 | 5 | 1 | 0 | 0 | 0 |

### The nvmod command (`api.cli`)

| Item | Id | State | Tests |
| --- | --- | --- | --- |
| --version prints the package version | `api.cli.version` | covered | `crates/nvmod-cli/tests/cli.rs::version_prints_the_package_version` |
| No arguments print the help, which lists the exit codes | `api.cli.help` | covered | `crates/nvmod-cli/tests/base.rs::the_help_lists_the_exit_codes`, `crates/nvmod-cli/tests/cli.rs::no_arguments_print_the_help` |
| An unknown command is a usage error | `api.cli.unknown_command` | covered | `crates/nvmod-cli/tests/cli.rs::an_unknown_command_is_a_usage_error` |
| Removed commands are unknown commands | `api.cli.removed_commands` | passing | `crates/nvmod-cli/tests/baseline.rs::removed_commands_are_unknown_commands` |
| Recorded command transcripts replay against the build | `api.cli.transcripts_replay` | passing | `crates/nvmod-cli/tests/baseline.rs::every_transcript_replays_against_the_build` |
| An unknown option or a missing value is an error | `api.cli.option_parsing` | covered | `crates/nvmod-cli/src/args.rs::unknown_options_and_missing_values_are_errors` |
| Each command leaves a redacted run log and keeps its output | `api.cli.run_log` | covered | `crates/nvmod-cli/tests/runlog.rs::a_command_on_an_instance_leaves_a_redacted_log_and_keeps_its_output` |
| A planted token and an outside path never reach the run log | `api.cli.run_log_redaction` | covered | `crates/nvmod-runlog/tests/canary.rs::a_planted_token_and_an_outside_path_never_reach_the_run_log` |
| The twenty-first run removes the oldest log | `api.cli.run_log_rotation` | covered | `crates/nvmod-runlog/tests/rotation.rs::the_twenty_first_run_removes_the_oldest` |
| --log-level is taken out wherever it stands | `api.cli.log_level` | covered | `crates/nvmod-cli/src/logging.rs::the_level_option_is_taken_out_wherever_it_is` |
| A secret never prints its value | `api.cli.secret_never_prints` | covered | `crates/nvmod-runlog/src/secret.rs::a_secret_never_prints_its_value` |
| Every command is available as a library call and through raul | `api.cli.run_outside_cli` | n/a |  |

### The nvmod-api jobs (`api.jobs`)

| Item | Id | State | Tests |
| --- | --- | --- | --- |
| Typed jobs for the Mods list and change, sort, plugins, install, saves, backup, stage and profile | `api.jobs.typed_jobs` | covered | `crates/nvmod-api/tests/jobs.rs::mods_list_gives_the_mods_in_priority_order`, `crates/nvmod-api/tests/jobs.rs::plugins_list_gives_the_plugins`, `crates/nvmod-api/tests/jobs.rs::stage_builds_the_tree_of_a_profile` |
| Jobs report progress events | `api.jobs.progress_events` | covered | `crates/nvmod-api/tests/jobs.rs::sort_proposes_an_order_and_reports_loot_rs_progress`, `crates/nvmod-api/tests/jobs.rs::the_first_progress_event_is_reading_the_masterlist` |
| A used cancel token stops every job before it touches the Instance | `api.jobs.cancel_before_start` | covered | `crates/nvmod-api/tests/jobs.rs::a_cancel_token_already_used_stops_every_job_before_it_touches_the_instance` |
| A cancelled job is a result and writes nothing | `api.jobs.cancel_is_a_result` | covered | `crates/nvmod-api/tests/jobs.rs::a_cancelled_sort_is_a_result_and_writes_nothing`, `crates/nvmod-api/tests/jobs.rs::a_sort_cancelled_while_loot_rs_runs_is_a_result_and_writes_nothing` |
| A refusal carries copyable text | `api.jobs.copyable_refusals` | covered | `crates/nvmod-api/tests/jobs.rs::a_missing_masterlist_is_a_refusal_with_copyable_text`, `crates/nvmod-api/tests/tree.rs::a_refusal_copies_as_text_without_a_command_line` |
| Every request round-trips through JSON and takes documented defaults | `api.jobs.json_requests` | covered | `crates/nvmod-api/tests/jobs.rs::a_request_without_its_optional_fields_takes_the_documented_defaults`, `crates/nvmod-api/tests/jobs.rs::every_request_round_trips_through_json` |
| An error kind has a stable code | `api.jobs.stable_error_codes` | covered | `crates/nvmod-api/tests/jobs.rs::an_error_kind_has_a_stable_code` |
| The API version is the release version | `api.jobs.api_version` | covered | `crates/nvmod-api/tests/jobs.rs::the_api_version_is_the_release_version` |
| A process-wide cancel does not stop a job with its own token | `api.jobs.scoped_cancel` | covered | `crates/nvmod-api/tests/cancel_scope.rs::a_process_wide_request_does_not_stop_a_job_with_its_own_token` |
| The Doctor job checks a game copy or an Instance and honours fail-on | `api.jobs.doctor_job` | covered | `crates/nvmod-api/tests/doctor.rs::a_base_copy_is_checked_and_reported_without_writing`, `crates/nvmod-api/tests/doctor.rs::a_mod_organizer_instance_is_checked_and_the_fail_on_severity_decides_found` |

### Checking a game copy (`doctor.base`)

| Item | Id | State | Tests |
| --- | --- | --- | --- |
| A Steam game copy gets a clean report | `doctor.base.steam_copy` | covered | `crates/nvmod-cli/tests/base.rs::a_steam_base_copy_gets_a_clean_report` |
| A GOG game copy gets a clean report | `doctor.base.gog_copy` | covered | `crates/nvmod-cli/tests/base.rs::a_gog_base_copy_gets_a_clean_report` |
| An Epic game copy is refused with a reason | `doctor.base.epic_refused` | covered | `crates/nvmod-cli/tests/base.rs::an_epic_base_copy_is_refused_with_a_reason` |
| A 4 GB patched executable is accepted | `doctor.base.four_gb_patch` | covered | `crates/nvmod-cli/tests/base.rs::a_4gb_patched_executable_is_accepted` |
| A missing executable or Data folder is reported | `doctor.base.missing_files` | covered | `crates/nvmod-cli/tests/base.rs::a_missing_executable_or_data_folder_is_reported` |
| doctor needs exactly one of --base and --instance | `doctor.base.one_target` | covered | `crates/nvmod-cli/tests/base.rs::doctor_needs_exactly_one_of_base_and_instance` |

### Checks of an Instance (`doctor.checks`)

| Item | Id | State | Tests |
| --- | --- | --- | --- |
| A healthy setup has no warnings or errors | `doctor.checks.healthy` | covered | `crates/nvmod-doctor/tests/doctor.rs::a_healthy_setup_has_no_warnings_or_errors` |
| Bad plugin files are each named | `doctor.checks.bad_plugin_files` | covered | `crates/nvmod-doctor/tests/doctor.rs::bad_plugin_files_are_each_named` |
| A DLL in two Mods is reported as shadowed | `doctor.checks.shadowed_dll` | covered | `crates/nvmod-doctor/tests/doctor.rs::a_dll_in_two_mods_is_reported_as_shadowed_and_the_first_mod_wins` |
| Catalogue rules: requirements, conflicts, replacements and ini settings | `doctor.checks.catalogue_rules` | covered | `crates/nvmod-doctor/tests/doctor.rs::catalogue_rules_requirements_conflicts_replacements_and_inis` |
| A missing master and a wrong load order are reported, and a sort fixes them | `doctor.checks.missing_master_wrong_order` | covered | `crates/nvmod-cli/tests/sort.rs::after_a_sort_the_doctor_finds_neither_a_missing_master_nor_a_wrong_order`, `crates/nvmod-cli/tests/sort.rs::the_doctor_reports_a_missing_master_and_a_wrong_load_order` |
| File times that no longer encode the order are noticed | `doctor.checks.file_times` | covered | `crates/nvmod-cli/tests/sort.rs::the_doctor_notices_file_times_that_no_longer_encode_the_order` |
| Problems of a Profile are reported | `doctor.checks.profile_problems` | covered | `crates/nvmod-doctor/tests/doctor.rs::profile_problems_are_reported` |
| A TTW Profile with the game's default ini gets one finding per key | `doctor.checks.ttw_ini_values` | covered | `crates/nvmod-cli/tests/builtin.rs::a_ttw_profile_with_the_game_default_ini_gets_one_finding_per_key` |
| The Doctor says when no Staging tree was built | `doctor.checks.no_staging_tree` | covered | `crates/nvmod-cli/tests/stage.rs::the_doctor_says_when_no_staging_tree_was_built` |
| Mods whose files collide after lower-casing are reported | `doctor.checks.lowercase_collisions` | covered | `crates/nvmod-cli/tests/stage.rs::the_doctor_reports_mods_whose_files_collide_after_lowercasing` |
| A tree a program wrote into and nobody swept is reported | `doctor.checks.unswept_tree` | covered | `crates/nvmod-cli/tests/sweep.rs::the_doctor_reports_an_unswept_tree` |
| A lock that a sort would violate is reported | `doctor.checks.lock_violation` | covered | `crates/nvmod-cli/tests/plugins.rs::the_doctor_reports_a_lock_that_sort_would_violate` |
| Base masters a Profile lacks are named and the repair fixes it | `doctor.checks.base_masters_repair` | covered | `crates/nvmod-cli/tests/base_masters.rs::the_doctor_names_base_masters_an_existing_profile_does_not_list_and_the_repair_fixes_it` |
| --fail-on sets the severity that makes the exit code non-zero | `doctor.checks.fail_on_threshold` | covered | `crates/nvmod-cli/tests/cli.rs::text_output_and_the_fail_on_threshold` |
| --json prints the report as stable JSON | `doctor.checks.json_report` | covered | `crates/nvmod-cli/tests/cli.rs::the_json_report_matches_the_golden_file` |
| The Doctor opens a schema 1 Instance without writing | `doctor.checks.old_instance_read_only` | covered | `crates/nvmod-cli/tests/instance_schema.rs::the_doctor_opens_a_schema_1_instance_without_writing` |

### NVSE plugins and the catalogue (`doctor.nvse`)

| Item | Id | State | Tests |
| --- | --- | --- | --- |
| pe inspects a plugin DLL without running it | `doctor.nvse.pe_inspection` | covered | `crates/nvmod-cli/tests/cli.rs::pe_describes_a_plugin_as_text_and_as_json` |
| pe says when a file is not an NVSE plugin | `doctor.nvse.pe_not_a_plugin` | covered | `crates/nvmod-cli/tests/cli.rs::pe_says_when_a_file_is_not_an_nvse_plugin` |
| catalog lists the plugin catalogue | `doctor.nvse.catalogue_listing` | covered | `crates/nvmod-cli/tests/cli.rs::catalog_lists_plugins_as_text_and_json` |
| Every catalogue entry carries its provenance | `doctor.nvse.catalogue_provenance` | covered | `crates/nvmod-catalog/src/lib.rs::the_shipped_catalogue_is_valid_and_every_entry_has_provenance` |
| A player's catalogue.d overlay adds entries marked as the player's | `doctor.nvse.overlay` | covered | `crates/nvmod-catalog/src/overlay.rs::an_overlay_entry_is_recognised_and_marked_as_user` |
| An overlay never shadows a shipped entry or claims more than external | `doctor.nvse.overlay_cannot_shadow` | covered | `crates/nvmod-catalog/src/overlay.rs::an_overlay_cannot_claim_more_than_external`, `crates/nvmod-catalog/src/overlay.rs::an_overlay_never_shadows_a_shipped_entry` |
| An overlay is bounded in entries, size and count | `doctor.nvse.overlay_caps` | covered | `crates/nvmod-catalog/src/overlay.rs::the_entry_cap_is_enforced` |
| On the vanilla target the extender DLL is required | `doctor.nvse.vanilla_needs_dll` | covered | `crates/nvmod-cli/tests/builtin.rs::on_the_vanilla_target_the_extender_dll_is_required_and_nothing_is_built_in` |
| On nv-rs a plugin DLL that is built in is redundant | `doctor.nvse.nv_rs_builtin_redundant` | covered | `crates/nvmod-cli/tests/builtin.rs::on_nv_rs_a_plugin_dll_that_is_built_in_is_redundant` |
| On nv-rs a plugin the catalogue marks absent cannot load | `doctor.nvse.nv_rs_absent_plugin` | covered | `crates/nvmod-cli/tests/builtin.rs::on_nv_rs_a_plugin_the_catalogue_marks_absent_is_unable_to_load_by_name` |
| Readiness per category from the catalogue and nv-rs's capabilities.json | `doctor.nvse.readiness` | covered | `crates/nvmod-cli/tests/builtin.rs::readiness_counts_each_category_from_the_catalogue_and_the_capabilities` |
| nvse-usage counts the opcodes the active plugins' scripts use | `doctor.nvse.opcode_usage` | covered | `crates/nvmod-cli/tests/nvse_usage.rs::planted_opcodes_are_counted_with_plugins_and_status` |
| nvse-usage prints only opcode names, counts and plugin files | `doctor.nvse.opcode_usage_privacy` | covered | `crates/nvmod-cli/tests/nvse_usage.rs::only_opcodes_names_counts_and_plugin_files_reach_the_output` |
| On the vanilla target the extender DLL is checked against the catalogue version | `doctor.nvse.extender_version` | covered | `crates/nvmod-cli/tests/builtin.rs::on_the_vanilla_target_the_extender_dll_is_checked_against_the_catalogue_version` |

### verify (`doctor.verify`)

| Item | Id | State | Tests |
| --- | --- | --- | --- |
| A clean Instance exits 0 with the Doctor report and the launch plan | `doctor.verify.clean` | covered | `crates/nvmod-cli/tests/verify.rs::a_clean_instance_exits_0_and_prints_the_doctor_report_and_the_launch_plan` |
| A Doctor error is exit 3 and the report still prints | `doctor.verify.doctor_error_exit` | covered | `crates/nvmod-cli/tests/verify.rs::a_doctor_error_is_exit_3_and_the_report_still_prints` |
| An nv-rs that matches its install record is verified | `doctor.verify.engine_matches_record` | covered | `crates/nvmod-cli/tests/verify.rs::an_nv_rs_that_matches_its_record_is_verified` |
| An nv-rs changed after its install fails verification | `doctor.verify.engine_changed` | covered | `crates/nvmod-cli/tests/verify.rs::an_nv_rs_changed_after_its_install_is_a_verification_failure` |
| verify writes nothing | `doctor.verify.writes_nothing` | covered | `crates/nvmod-cli/tests/verify.rs::verify_writes_nothing` |

### BSA archives (`formats.archives`)

| Item | Id | State | Tests |
| --- | --- | --- | --- |
| Names are lower-cased with forward slashes | `formats.archives.read_names` | covered | `crates/nvmod-bsa/tests/reader.rs::names_are_lowercased_and_use_slashes` |
| The compression flag and the toggle bit decide compressed | `formats.archives.compression` | covered | `crates/nvmod-bsa/tests/reader.rs::the_compression_flag_and_the_toggle_bit_decide_compressed` |
| A wrong signature or version, truncated input and oversized counts are refused | `formats.archives.bad_header_refused` | covered | `crates/nvmod-bsa/tests/reader.rs::a_wrong_signature_or_version_is_named`, `crates/nvmod-bsa/tests/reader.rs::oversized_counts_are_refused`, `crates/nvmod-bsa/tests/reader.rs::truncated_input_is_refused_at_every_length` |
| A two-gigabyte class archive costs only its directory | `formats.archives.large_archive_cheap` | covered | `crates/nvmod-bsa/tests/reader.rs::a_two_gigabyte_class_archive_costs_only_its_directory` |
| Mutated archives never panic | `formats.archives.never_panics` | covered | `crates/nvmod-bsa/tests/reader.rs::mutated_archives_never_panic` |
| Writing BSA archives | `formats.archives.packing` | n/a |  |

### Text, ini and binary file readers (`formats.files`)

| Item | Id | State | Tests |
| --- | --- | --- | --- |
| plugins.txt is read with names, comments, stars and a code page | `formats.files.plugin_lists` | covered | `crates/nvmod-formats/src/plugins.rs::a_code_page_name_is_decoded` |
| The locked-order file round-trips with its header and CRLF | `formats.files.locked_order` | covered | `crates/nvmod-formats/src/lockedorder.rs::a_file_round_trips_with_its_header_and_crlf`, `crates/nvmod-formats/src/lockedorder.rs::bad_lines_and_non_utf8_are_refused` |
| Ini sections, keys and Qt strings are read | `formats.files.ini_reader` | covered | `crates/nvmod-formats/src/ini.rs::qt_strings`, `crates/nvmod-formats/src/ini.rs::sections_keys_and_wrappers` |
| Setting a key changes only its line | `formats.files.meta_ini_edit` | covered | `crates/nvmod-formats/src/metaini.rs::setting_a_key_changes_only_its_line` |
| A save game is read; truncation is an error, not a panic | `formats.files.save_game` | covered | `crates/nvmod-formats/src/savegame.rs::a_synthetic_save_is_read`, `crates/nvmod-formats/src/savegame.rs::every_truncation_is_an_error_not_a_panic` |
| Scripts of a plugin are scanned for opcodes, including compressed records | `formats.files.script_scan` | covered | `crates/nvmod-formats/src/scripts.rs::a_compressed_record_is_read`, `crates/nvmod-formats/src/scripts.rs::statements_expressions_and_nested_arguments_are_counted` |
| Text is UTF-8 or Windows-1252, and encoding refuses what the code page lacks | `formats.files.text_code_pages` | covered | `crates/nvmod-formats/src/text.rs::encoding_is_the_inverse_of_decoding_and_refuses_what_the_code_page_lacks`, `crates/nvmod-formats/src/text.rs::invalid_utf8_is_read_as_windows_1252` |
| Dotted, suffixed and date versions order | `formats.files.version_order` | covered | `crates/nvmod-formats/src/version.rs::dotted_suffixed_and_date_versions_order` |
| A Windows drive path maps to a host path | `formats.files.wine_paths` | covered | `crates/nvmod-formats/src/winpath.rs::other_drives_need_a_mapping`, `crates/nvmod-formats/src/winpath.rs::z_maps_to_the_root_in_either_slash_style` |
| categories.dat names are read and its bytes survive | `formats.files.categories_dat` | covered | `crates/nvmod-formats/src/categories.rs::names_are_read_and_the_bytes_survive` |
| A directory scan does not follow symlinks and bounds its depth | `formats.files.scan_bounded` | passing | `crates/nvmod-formats/src/scan.rs::symlinks_are_not_followed_and_depth_is_bounded` |

### PE files (`formats.pe`)

| Item | Id | State | Tests |
| --- | --- | --- | --- |
| An NVSE plugin is recognised with its exports and imports | `formats.pe.plugin_exports` | covered | `crates/nvmod-pe/tests/inspect.rs::an_nvse_plugin_is_recognised_with_its_exports_and_imports` |
| The plugin's version struct is read from its exported data | `formats.pe.version_struct` | covered | `crates/nvmod-pe/tests/inspect.rs::the_version_struct_is_read_from_the_exported_data` |
| The version resource and the large-address-aware flag are read | `formats.pe.large_address_flag` | covered | `crates/nvmod-pe/tests/inspect.rs::the_version_resource_and_the_large_address_flag_are_read` |
| A 64-bit DLL is read but is not a 32-bit image | `formats.pe.sixty_four_bit` | covered | `crates/nvmod-pe/tests/inspect.rs::a_64_bit_dll_is_read_but_is_not_a_32_bit_x86_image` |
| A damaged table is an issue and the rest still reads | `formats.pe.damaged_tables` | covered | `crates/nvmod-pe/tests/inspect.rs::a_damaged_export_table_is_an_issue_and_the_rest_still_reads` |
| A file that is not a PE file is refused with a reason | `formats.pe.not_pe_refused` | covered | `crates/nvmod-pe/tests/inspect.rs::files_that_are_not_pe_files_are_refused_with_a_reason` |
| Hostile input never panics and reads stay inside the file | `formats.pe.hostile_input_bounded` | covered | `crates/nvmod-pe/tests/properties.rs::arbitrary_bytes_never_panic`, `crates/nvmod-pe/tests/properties.rs::changing_any_bytes_of_a_valid_file_never_panics_and_reads_stay_in_the_file` |

### Creating an Instance (`instance.init`)

| Item | Id | State | Tests |
| --- | --- | --- | --- |
| init builds an Instance from a game copy | `instance.init.init_from_base` | covered | `crates/nvmod-cli/tests/base.rs::init_builds_an_instance_skeleton_the_doctor_accepts` |
| init refuses an unsupported game copy and writes nothing | `instance.init.init_refuses_unsupported_base` | covered | `crates/nvmod-cli/tests/base.rs::init_refuses_an_unsupported_base_and_writes_nothing` |
| init never overwrites an Instance, nor makes one inside the game folder | `instance.init.init_never_overwrites` | covered | `crates/nvmod-cli/tests/base.rs::init_refuses_to_overwrite_an_instance_and_one_inside_the_base` |
| Steam and GOG game copies are supported; others are refused with a reason | `instance.init.storefronts` | covered | `crates/nvmod-instance/src/storefront.rs::each_fixture_is_detected_and_only_steam_and_gog_are_supported` |
| The game folder is never written to | `instance.init.game_folder_untouched` | covered | `crates/nvmod-cli/tests/base.rs::doctor_never_writes_to_the_base_copy`, `crates/nvmod-cli/tests/stage.rs::staging_never_changes_the_base_copy_or_the_mods` |
| A new Profile lists the game's masters that are present, in game order | `instance.init.base_masters_listed` | covered | `crates/nvmod-cli/tests/base_masters.rs::a_new_profile_lists_every_master_of_the_base_copy_active_and_in_game_order` |

### Instance file and schemas (`instance.model`)

| Item | Id | State | Tests |
| --- | --- | --- | --- |
| An Instance file carries a schema version and round-trips | `instance.model.current_schema` | covered | `crates/nvmod-core/src/model.rs::a_current_instance_round_trips`, `crates/nvmod-instance/src/instance.rs::create_then_open_round_trips` |
| A schema 1 Instance migrates on its first write, with a backup | `instance.model.schema_migration` | covered | `crates/nvmod-cli/tests/instance_schema.rs::a_schema_1_instance_is_migrated_by_the_first_write_with_a_backup`, `crates/nvmod-instance/src/instance.rs::a_schema_1_file_opens_read_only_and_migrates_on_open_with_a_backup` |
| An Instance file from a newer nvmod is refused and left alone | `instance.model.newer_schema_refused` | covered | `crates/nvmod-cli/tests/instance_schema.rs::a_newer_schema_is_refused_with_exit_2_and_a_reason_and_left_alone`, `crates/nvmod-core/src/model.rs::a_newer_instance_is_refused_with_a_reason` |
| The JSON schemas of the Instance and Profile are checked in and match the code | `instance.model.json_schemas` | covered | `crates/nvmod-core/src/model.rs::the_checked_in_instance_schema_is_the_generated_one`, `crates/nvmod-core/src/model.rs::the_checked_in_profile_schema_is_the_generated_one` |
| Windows paths in an Instance map to this machine | `instance.model.windows_paths` | covered | `crates/nvmod-formats/tests/instance.rs::the_instance_is_read_with_its_paths_mapped_to_this_machine` |

### Profiles (`instance.profiles`)

| Item | Id | State | Tests |
| --- | --- | --- | --- |
| Create a Profile | `instance.profiles.create` | covered | `crates/nvmod-api/tests/jobs.rs::profile_create_makes_an_empty_profile_with_the_master_as_its_only_plugin` |
| Copy a Profile, with or without its saves | `instance.profiles.copy` | covered | `crates/nvmod-cli/tests/profile_ops.rs::copy_with_saves_makes_an_equal_tree_and_leaves_the_original_alone`, `crates/nvmod-cli/tests/profile_ops.rs::copy_without_saves_leaves_the_saves_out` |
| Rename a Profile | `instance.profiles.rename` | covered | `crates/nvmod-cli/tests/profile_ops.rs::rename_moves_the_profile_and_is_undone_by_rollback` |
| Delete a Profile into a trash that rollback can undo | `instance.profiles.delete_to_trash` | covered | `crates/nvmod-cli/tests/profile_ops.rs::delete_moves_the_profile_to_the_trash_and_rollback_brings_it_back` |
| Select the Profile that commands use by default | `instance.profiles.select` | covered | `crates/nvmod-cli/tests/profile_ops.rs::select_records_the_profile_in_the_instance_file_and_decides_the_default` |
| The selected Profile cannot be deleted or renamed | `instance.profiles.selected_protected` | covered | `crates/nvmod-cli/tests/profile_ops.rs::the_selected_profile_cannot_be_deleted_or_renamed` |
| Profile switches (LocalSaves, LocalSettings, AutomaticArchiveInvalidation) | `instance.profiles.settings` | covered | `crates/nvmod-cli/tests/profile_ops.rs::set_edits_one_line_and_the_reader_returns_it` |
| Unusable Profile names are refused with a reason | `instance.profiles.name_rules` | covered | `crates/nvmod-instance/src/profile_name.rs::unusable_names_are_refused_with_a_reason` |
| An Instance targets vanilla or nv-rs | `instance.profiles.target_switch` | covered | `crates/nvmod-api/tests/jobs.rs::target_set_switches_the_instance_to_nv_rs` |

### The downloads library (`mods.downloads`)

| Item | Id | State | Tests |
| --- | --- | --- | --- |
| List the downloaded archives with their state | `mods.downloads.list` | covered | `crates/nvmod-cli/tests/downloads.rs::list_json_matches_its_schema_and_names_with_brackets_work` |
| Show one archive, with optional hashes | `mods.downloads.show_hash` | covered | `crates/nvmod-cli/tests/downloads.rs::show_with_hash_and_a_path_inside_the_folder` |
| Install from the library marks the archive installed | `mods.downloads.install_from_library` | covered | `crates/nvmod-api/tests/downloads.rs::installing_from_the_library_equals_installing_the_path_plus_the_meta_update`, `crates/nvmod-cli/tests/downloads.rs::install_marks_the_meta_and_hide_unhide_forget_work` |
| List archives with no Mod and Mods with no archive | `mods.downloads.unmatched` | covered | `crates/nvmod-downloads/tests/library.rs::unmatched_lists_archives_with_no_mod_and_mods_with_no_archive` |
| Forgetting an archive moves it and its meta to the trash | `mods.downloads.forget_to_trash` | covered | `crates/nvmod-downloads/tests/library.rs::forgetting_moves_the_archive_and_its_meta_to_the_trash` |
| The downloads folder can be another one | `mods.downloads.other_folder` | covered | `crates/nvmod-cli/tests/downloads.rs::the_downloads_option_names_another_folder` |
| Download a mod from Nexus | `mods.downloads.nexus_download` | deferred |  |
| Show which Mods have a newer version | `mods.downloads.update_rows` | deferred |  |

### FOMOD installers (`mods.fomod`)

| Item | Id | State | Tests |
| --- | --- | --- | --- |
| A FOMOD with inferable choices installs without questions | `mods.fomod.inferred_choices` | covered | `crates/nvmod-cli/tests/install.rs::a_fomod_with_inferable_choices_installs_without_questions` |
| A choice that cannot be inferred becomes a question and nothing is written | `mods.fomod.questions` | covered | `crates/nvmod-cli/tests/install.rs::a_fomod_choice_that_cannot_be_inferred_is_put_as_a_question_and_nothing_is_written` |
| An answers file completes the install | `mods.fomod.answers_file` | covered | `crates/nvmod-cli/tests/install.rs::an_answers_file_completes_the_install` |
| Bad answers are refused and change nothing | `mods.fomod.bad_answers_refused` | covered | `crates/nvmod-cli/tests/install.rs::bad_answers_are_refused_and_change_nothing` |
| A terminal session asks the questions and saves the answers | `mods.fomod.terminal_session` | covered | `crates/nvmod-cli/tests/fomod_interactive.rs::a_scripted_stdin_session_installs_exactly_the_planned_files_and_saves_the_answers` |
| reinstall replays the answers and reconfigure changes only what one changed answer touches | `mods.fomod.reinstall_reconfigure` | covered | `crates/nvmod-cli/tests/fomod_interactive.rs::reinstall_replays_and_reconfigure_with_one_changed_answer_changes_only_those_files` |
| The same answers always give the same plan | `mods.fomod.same_answers_same_plan` | covered | `crates/nvmod-cli/tests/fomod_interactive.rs::the_same_answers_always_yield_the_same_plan` |
| A destination that climbs out of the Mod folder is refused | `mods.fomod.destination_escape_refused` | covered | `crates/nvmod-cli/tests/fomod_interactive.rs::a_destination_that_climbs_out_of_the_mod_folder_is_refused_and_writes_nothing` |
| The step-by-step event stream follows its published schema | `mods.fomod.event_stream` | covered | `crates/nvmod-api/tests/jobs.rs::the_fomod_stream_gives_the_step_then_the_plan_and_the_events_follow_the_schema` |
| A step shown only by a flag is skipped or asked about | `mods.fomod.conditional_steps` | covered | `crates/nvmod-modinstall/src/fomod.rs::a_step_shown_only_by_a_flag_is_skipped_or_asked_about` |
| FOMOD XML in UTF-16 or with a byte-order mark is read | `mods.fomod.xml_encodings` | covered | `crates/nvmod-modinstall/src/fomod.rs::xml_in_utf16_or_with_a_byte_order_mark_is_read` |

### Installing a mod (`mods.install`)

| Item | Id | State | Tests |
| --- | --- | --- | --- |
| A plain archive is installed enabled at the highest priority and staged | `mods.install.archive` | covered | `crates/nvmod-cli/tests/install.rs::a_plain_archive_is_placed_enabled_at_highest_priority_and_staged` |
| Zip, 7z, tar and compressed tar archives are read in process | `mods.install.archive_formats` | covered | `crates/nvmod-modinstall/src/extract.rs::every_supported_format_is_read_in_process_and_rar_is_not` |
| A tar archive installs | `mods.install.tar` | covered | `crates/nvmod-cli/tests/install.rs::a_tar_archive_installs_too` |
| A folder installs like an archive and is left as it was | `mods.install.folder` | covered | `crates/nvmod-cli/tests/install.rs::a_folder_is_installed_like_an_archive_and_left_as_it_was` |
| A wrapper folder or a Data folder is stripped | `mods.install.wrapper_stripped` | covered | `crates/nvmod-cli/tests/install.rs::a_wrapper_folder_and_a_data_folder_are_stripped` |
| A RAR archive is refused with advice and nothing is written | `mods.install.rar_refused` | covered | `crates/nvmod-cli/tests/install.rs::a_rar_archive_is_refused_with_advice_and_nothing_is_written` |
| A Nexus-named archive writes its provenance into meta.ini | `mods.install.nexus_provenance` | covered | `crates/nvmod-cli/tests/install.rs::a_nexus_named_archive_writes_its_provenance_into_meta_ini` |
| An existing Mod name is refused | `mods.install.duplicate_name_refused` | covered | `crates/nvmod-cli/tests/install.rs::an_existing_mod_name_is_refused_and_nothing_changes` |
| plugins.txt is written in the Windows code page, loadorder.txt in UTF-8 | `mods.install.plugin_lists_encoding` | covered | `crates/nvmod-cli/tests/install.rs::plugins_txt_is_written_in_the_windows_code_page_and_loadorder_in_utf8` |
| Hostile archives (traversal, absolute paths, links, bombs) are refused and change nothing | `mods.install.hostile_archives_refused` | covered | `crates/nvmod-cli/tests/install.rs::hostile_archives_are_refused_and_change_nothing`, `crates/nvmod-modinstall/tests/in_process_archives.rs::traversal_is_refused_and_nothing_leaves_dest` |
| Entry count, size, ratio, depth and time limits bound every extraction | `mods.install.extraction_limits` | covered | `crates/nvmod-cli/tests/install.rs::every_extraction_limit_refuses_with_exit_2_names_itself_and_leaves_the_instance_alone`, `crates/nvmod-modinstall/tests/bounded_extraction.rs::an_expansion_bomb_is_refused_and_a_ratio_just_over_it_is_not` |
| A dry run lists the changes and writes nothing | `mods.install.dry_run` | covered | `crates/nvmod-cli/tests/install.rs::dry_run_lists_the_changes_and_writes_nothing` |
| rollback undoes the last change | `mods.install.rollback` | covered | `crates/nvmod-cli/tests/install.rs::rollback_restores_the_previous_state_and_removes_the_mod` |
| A failed step rolls back by itself | `mods.install.failed_step_rolls_back` | covered | `crates/nvmod-cli/tests/install.rs::a_failed_step_rolls_back_by_itself_with_exit_4` |

### The life of a Mod (`mods.lifecycle`)

| Item | Id | State | Tests |
| --- | --- | --- | --- |
| List the Mods in priority order, with their categories | `mods.lifecycle.list_priority` | covered | `crates/nvmod-api/tests/jobs.rs::a_mods_list_with_a_category_keeps_only_that_category`, `crates/nvmod-cli/tests/mods.rs::list_prints_the_priority_order_as_json_and_text` |
| Turn Mods on and off | `mods.lifecycle.enable_disable` | covered | `crates/nvmod-cli/tests/mods.rs::disable_and_enable_edit_only_modlist_stage_again_and_roll_back` |
| Change which Mod wins | `mods.lifecycle.reorder` | covered | `crates/nvmod-cli/tests/mods.rs::reorder_moves_one_entry`, `crates/nvmod-cli/tests/mods_life.rs::reordering_across_separators_keeps_the_separator_lines` |
| Remove a Mod; rollback brings it back | `mods.lifecycle.remove` | covered | `crates/nvmod-cli/tests/mods_life.rs::remove_then_rollback_restores_every_file_and_list_line` |
| Removing a Mod other plugins need names the dependants and needs --force | `mods.lifecycle.remove_dependants` | covered | `crates/nvmod-cli/tests/mods_life.rs::removing_a_mod_other_plugins_need_names_the_dependants_and_needs_force` |
| Rename a Mod | `mods.lifecycle.rename` | covered | `crates/nvmod-cli/tests/mods_life.rs::rename_updates_the_folder_and_the_line_and_refuses_an_existing_name` |
| Create an empty Mod | `mods.lifecycle.create` | covered | `crates/nvmod-cli/tests/mods_life.rs::create_makes_an_empty_mod_at_the_place_and_roll_back_removes_it` |
| Clone a Mod, optionally with hard links | `mods.lifecycle.clone` | covered | `crates/nvmod-cli/tests/mods_life.rs::a_clone_with_another_name_is_listed_disabled_below_the_original_and_can_hard_link` |
| Hide and unhide one file of a Mod | `mods.lifecycle.hide_unhide` | covered | `crates/nvmod-cli/tests/mods_life.rs::hide_and_unhide_round_trip_a_file_and_the_staging_tree_follows` |
| Notes on a Mod are kept in meta.ini | `mods.lifecycle.notes` | covered | `crates/nvmod-cli/tests/mods_life.rs::a_note_is_written_to_meta_ini_and_rolled_back` |
| Separators in MO2's form | `mods.lifecycle.separators` | covered | `crates/nvmod-cli/tests/mods_life.rs::a_separator_has_mo2s_form_and_removing_it_leaves_the_other_lines` |
| Categories are set, listed and filtered | `mods.lifecycle.categories` | covered | `crates/nvmod-cli/tests/mods_life.rs::categories_are_set_listed_filtered_and_the_file_is_untouched` |
| Disabling a Mod takes out the plugins only it provides, never base plugins | `mods.lifecycle.disable_deactivates_plugins` | covered | `crates/nvmod-cli/tests/mods_disable_plugins.rs::disabling_a_mod_that_ships_base_masters_takes_out_only_what_no_one_else_provides` |
| Enabling a Mod puts back the plugins its disable took out | `mods.lifecycle.enable_restores_plugins` | covered | `crates/nvmod-cli/tests/mods_disable_plugins.rs::disable_then_enable_is_the_identity_on_the_profile_files_and_the_plugin_report` |
| A running game of the Instance refuses a change, not a dry run | `mods.lifecycle.running_game_refuses_change` | covered | `crates/nvmod-cli/tests/mods_life.rs::a_running_game_of_the_instance_refuses_a_change_but_not_a_dry_run` |
| A failing step puts everything back | `mods.lifecycle.failed_step_restores` | covered | `crates/nvmod-cli/tests/mods_life.rs::a_failing_step_puts_everything_back_and_exits_4` |
| modlist.txt prefixes, separators and foreign entries are read as MO2 writes them | `mods.lifecycle.modlist_format` | covered | `crates/nvmod-formats/src/modlist.rs::prefixes_separators_and_foreign_entries_are_read` |
| meta.ini comes back byte for byte when nothing changed | `mods.lifecycle.meta_ini_kept` | covered | `crates/nvmod-formats/tests/metaini.rs::parse_then_write_is_byte_identical` |

### The Overwrite folder (`mods.overwrite`)

| Item | Id | State | Tests |
| --- | --- | --- | --- |
| List the files in Overwrite with the Mods that hold the same path | `mods.overwrite.list` | covered | `crates/nvmod-cli/tests/overwrite.rs::list_shows_each_file_with_the_mods_that_provide_the_same_path` |
| Move all or matching files into a new Mod | `mods.overwrite.create_mod` | covered | `crates/nvmod-cli/tests/overwrite.rs::create_mod_with_a_glob_moves_only_matches_and_keeps_the_folders` |
| Move a file or folder into a Mod | `mods.overwrite.move_into_mod` | covered | `crates/nvmod-cli/tests/overwrite.rs::move_puts_a_folder_into_a_mod_and_refuses_a_name_the_mod_has` |
| Delete into a trash that rollback can undo | `mods.overwrite.delete_to_trash` | covered | `crates/nvmod-cli/tests/overwrite.rs::delete_moves_to_the_trash_and_rollback_brings_it_back` |
| sync moves what has one provider and refuses an ambiguous file | `mods.overwrite.sync` | covered | `crates/nvmod-cli/tests/overwrite.rs::sync_moves_what_has_one_provider_and_refuses_an_ambiguous_one_without_to` |
| A failure in the middle puts everything back | `mods.overwrite.failure_puts_back` | covered | `crates/nvmod-cli/tests/overwrite.rs::a_failure_in_the_middle_puts_everything_back` |

### Plugins (`plugins.order`)

| Item | Id | State | Tests |
| --- | --- | --- | --- |
| List plugins with position, masters and provider | `plugins.order.list` | covered | `crates/nvmod-cli/tests/plugins.rs::list_shows_position_masters_provider_and_the_forced_plugin` |
| Enabling a plugin whose master is inactive is refused unless --with-masters | `plugins.order.enable_with_masters` | covered | `crates/nvmod-cli/tests/plugins.rs::enabling_a_plugin_whose_master_is_inactive_is_refused_unless_with_masters` |
| A master that is not installed cannot be enabled | `plugins.order.missing_master_refused` | covered | `crates/nvmod-cli/tests/plugins.rs::a_master_that_is_not_installed_cannot_be_enabled_even_with_masters` |
| Disabling a master an active plugin needs names the dependants | `plugins.order.disable_names_dependants` | covered | `crates/nvmod-cli/tests/plugins.rs::disabling_a_master_that_an_active_plugin_needs_names_the_dependants` |
| The game's own plugins are forced first and cannot be moved, disabled or locked | `plugins.order.forced_plugins` | covered | `crates/nvmod-cli/tests/plugins.rs::forced_primary_plugins_cannot_be_moved_disabled_or_locked` |
| The Tale of Two Wastelands primary plugins are forced first for a TTW Profile | `plugins.order.ttw_forced_plugins` | covered | `crates/nvmod-cli/tests/plugins.rs::ttw_primary_plugins_are_forced_first_for_a_ttw_profile`, `crates/nvmod-loadorder/src/lib.rs::the_ttw_list_is_forced_only_when_the_list_has_the_ttw_master` |
| Move a plugin, never breaking master order | `plugins.order.move_plugin` | covered | `crates/nvmod-cli/tests/plugins.rs::move_changes_both_lists_and_refuses_to_break_master_order` |
| Lock a plugin to a position, and unlock it | `plugins.order.lock_unlock` | covered | `crates/nvmod-cli/tests/plugins.rs::locks_are_written_kept_for_absent_plugins_and_rolled_back` |
| A locked plugin keeps its position after a sort | `plugins.order.lock_survives_sort` | covered | `crates/nvmod-cli/tests/plugins.rs::a_locked_plugin_keeps_its_position_after_sort_apply`, `crates/nvmod-loadorder/src/lib.rs::a_locked_plugin_goes_back_to_its_position_after_a_sort` |
| plugins.txt is Windows-1252; a name the code page lacks is refused | `plugins.order.code_page` | covered | `crates/nvmod-cli/tests/plugins.rs::plugins_txt_is_windows_1252_and_a_name_the_code_page_lacks_is_refused` |
| CRLF and the header round-trip, and both lists roll back byte for byte | `plugins.order.lists_round_trip` | covered | `crates/nvmod-cli/tests/plugins.rs::crlf_and_the_header_round_trip`, `crates/nvmod-cli/tests/plugins.rs::plugins_txt_and_loadorder_txt_roll_back_byte_for_byte_after_one_step` |
| A move restamps the plugin file times of a staged Profile | `plugins.order.file_times_restamped` | covered | `crates/nvmod-cli/tests/plugins.rs::a_move_restamps_the_plugin_file_times_of_a_staged_profile` |

### Sorting the load order (`plugins.sort`)

| Item | Id | State | Tests |
| --- | --- | --- | --- |
| sort prints a proposal and writes nothing without --apply | `plugins.sort.proposal_only` | covered | `crates/nvmod-cli/tests/sort.rs::the_proposal_is_printed_and_nothing_is_written_without_apply` |
| --apply writes both lists and encodes the order in file times | `plugins.sort.apply` | covered | `crates/nvmod-cli/tests/sort.rs::apply_writes_both_lists_and_encodes_the_order_in_file_times` |
| Applying twice gives the same order and changes nothing | `plugins.sort.idempotent` | covered | `crates/nvmod-cli/tests/sort.rs::running_apply_twice_gives_the_same_order_and_changes_nothing` |
| rollback undoes a sort | `plugins.sort.rollback` | covered | `crates/nvmod-cli/tests/sort.rs::rollback_undoes_the_sort` |
| --mode minimal moves only what a rule forces | `plugins.sort.minimal_mode` | covered | `crates/nvmod-cli/tests/sort.rs::the_minimal_mode_moves_only_what_a_rule_forces_and_names_its_mode` |
| Rules that contradict each other are refused | `plugins.sort.contradicting_rules_refused` | covered | `crates/nvmod-cli/tests/sort.rs::rules_that_contradict_each_other_are_refused` |
| The pinned LOOT masterlist is fetched, verified, stored and used | `plugins.sort.pinned_masterlist` | covered | `crates/nvmod-cli/tests/masterlist.rs::the_pinned_masterlist_is_fetched_verified_stored_and_used` |
| A masterlist that does not match its checksum is not used | `plugins.sort.masterlist_checksum` | covered | `crates/nvmod-cli/tests/masterlist.rs::a_masterlist_that_does_not_match_its_checksum_exits_3_and_is_not_used` |
| A plugin's requirement on an extender is one error line, or a note on nv-rs | `plugins.sort.requirement_messages` | covered | `crates/nvmod-cli/tests/sort_requirements.rs::on_nv_rs_a_built_in_extender_is_a_note_with_its_coverage_and_the_rest_keeps_its_message`, `crates/nvmod-cli/tests/sort_requirements.rs::on_vanilla_each_requirement_is_one_error_line_and_nothing_is_built_in` |
| keep-order stamps the file times and leaves the order alone | `plugins.sort.keep_order` | covered | `crates/nvmod-cli/tests/sort.rs::keep_order_stamps_the_times_and_leaves_the_order_alone` |
| A sort reports each stage of loot-rs with counts | `plugins.sort.progress_stages` | covered | `crates/nvmod-api/tests/jobs.rs::a_sort_reports_every_stage_with_counts_and_a_stable_label` |
| A run fixes plugin times that do not follow the load order | `plugins.sort.run_fixes_times` | covered | `crates/nvmod-cli/tests/sort.rs::run_fixes_plugin_times_that_do_not_follow_the_load_order_before_the_game_starts` |
| A Profile without a Staging tree is refused with the stage command | `plugins.sort.needs_staging` | covered | `crates/nvmod-cli/tests/sort.rs::a_profile_without_a_staging_tree_is_refused_with_the_stage_command` |

### Starting the game and other programs (`run.launch`)

| Item | Id | State | Tests |
| --- | --- | --- | --- |
| A dry run prints the plan and changes nothing | `run.launch.dry_run_plan` | passing | `crates/nvmod-cli/tests/run.rs::dry_run_prints_the_plan_and_changes_nothing` |
| On Linux the game runs through Proton with the prefix, app id and staged data | `run.launch.proton` | passing | `crates/nvmod-cli/tests/run.rs::run_goes_through_proton_with_the_prefix_app_id_folder_and_staged_data` |
| On an nv-rs Instance the game runs natively and nothing is copied | `run.launch.nv_rs_native` | passing | `crates/nvmod-cli/tests/run.rs::game_on_an_nv_rs_instance_is_native_and_via_proton_is_the_fallback`, `crates/nvmod-cli/tests/run.rs::nv_rs_runs_natively_from_a_run_folder_and_nothing_is_copied` |
| Only the program that is named is started | `run.launch.named_program` | covered | `crates/nvmod-cli/tests/run.rs::a_named_program_is_planned_and_the_dry_run_writes_nothing`, `crates/nvmod-run/src/native.rs::only_a_named_program_is_found_and_the_refusals_name_no_command` |
| The game entry agrees with verify | `run.launch.game_agrees_with_verify` | passing | `crates/nvmod-cli/tests/run.rs::the_game_entry_agrees_with_verify` |
| A live lock refuses every writing command; a dead one is taken over | `run.launch.instance_lock` | passing | `crates/nvmod-cli/tests/run.rs::a_live_lock_refuses_every_writing_command_and_a_dead_one_is_taken_over` |
| A detached run keeps the Instance locked until the program ends | `run.launch.detached_run_locks` | passing | `crates/nvmod-cli/tests/run.rs::a_detached_run_keeps_the_instance_locked_until_the_program_ends` |
| The Profile is placed in the prefix before the start and collected after | `run.launch.profile_placed_collected` | passing | `crates/nvmod-cli/tests/run.rs::the_profile_is_placed_in_the_prefix_before_the_start_and_collected_after` |
| A launcher of the caller starts the program and nothing else does | `run.launch.callers_launcher` | passing | `crates/nvmod-cli/tests/run.rs::a_launcher_of_the_callers_starts_the_program_and_nothing_else_does` |
| A running Mod Organizer refuses the run | `run.launch.mo2_running_refuses` | passing | `crates/nvmod-cli/tests/run.rs::a_running_mo2_refuses_the_run` |
| Starting the real game on a real install | `run.launch.real_game_start` | gated |  |

### Placing a Profile in the game's prefix (`run.prefix`)

| Item | Id | State | Tests |
| --- | --- | --- | --- |
| apply places the Profile in the prefix | `run.prefix.apply_places_profile` | covered | `crates/nvmod-cli/tests/profile_apply.rs::apply_reports_json_and_places_the_profile` |
| LocalSettings places the five inis and off puts the player's own back | `run.prefix.local_settings` | covered | `crates/nvmod-profile/tests/apply.rs::local_settings_place_the_five_inis_and_off_puts_the_users_own_back` |
| Ini tweaks of the Mods merge in priority order | `run.prefix.initweaks_merge` | covered | `crates/nvmod-profile/tests/apply.rs::initweaks_merge_in_priority_order_with_the_profile_tweaks_last` |
| LocalSaves redirects the save folder and restores it byte for byte | `run.prefix.local_saves_redirect` | covered | `crates/nvmod-profile/tests/apply.rs::local_saves_redirect_and_restore_byte_for_byte` |
| Applying twice changes nothing and A, B, A leaves the prefix as the first A | `run.prefix.apply_idempotent` | covered | `crates/nvmod-profile/tests/apply.rs::applying_twice_changes_nothing_and_a_b_a_leaves_the_prefix_as_the_first_a` |
| finish collects what the game wrote | `run.prefix.finish_collects` | covered | `crates/nvmod-cli/tests/profile_apply.rs::finish_collects_what_the_game_wrote` |
| A run that was cut off is put back by the next one | `run.prefix.cut_off_run_recovered` | covered | `crates/nvmod-profile/tests/apply.rs::a_run_that_was_cut_off_is_put_back_by_the_next_one` |
| The invalidation archive reaches a staged tree | `run.prefix.archive_invalidation` | covered | `crates/nvmod-cli/tests/profile_apply.rs::the_invalidation_archive_reaches_a_staged_tree` |
| A dry run writes nothing and a missing prefix is refused | `run.prefix.dry_run` | covered | `crates/nvmod-cli/tests/profile_apply.rs::a_dry_run_writes_nothing_and_a_missing_prefix_is_refused` |
| Nothing is written outside the Instance and the named prefix | `run.prefix.nothing_outside` | covered | `crates/nvmod-profile/tests/apply.rs::nothing_is_written_outside_the_instance_and_the_named_prefix` |

### Programs registry (`run.registry`)

| Item | Id | State | Tests |
| --- | --- | --- | --- |
| exec add, edit and remove round-trip | `run.registry.add_edit_remove` | passing | `crates/nvmod-cli/tests/run.rs::exec_add_edit_and_remove_round_trip` |
| Titles are unique ignoring case and not reserved | `run.registry.unique_titles` | covered | `crates/nvmod-run/src/registry.rs::titles_are_unique_without_case_and_not_reserved` |
| import lists mapped paths and refuses other drives | `run.registry.import_mapped_paths` | passing | `crates/nvmod-cli/tests/run.rs::import_lists_mapped_paths_and_refuses_other_drives_with_a_finding` |

### Profile backups (`safety.backup`)

| Item | Id | State | Tests |
| --- | --- | --- | --- |
| Create, change, restore gives the files back byte for byte | `safety.backup.round_trip` | covered | `crates/nvmod-cli/tests/backup.rs::create_change_restore_gives_the_files_back_byte_for_byte` |
| Saves are in a snapshot only on request | `safety.backup.saves_on_request` | covered | `crates/nvmod-cli/tests/backup.rs::snapshots_are_real_copies_of_text_and_inis_and_saves_only_on_request` |
| A damaged snapshot is refused and nothing changes | `safety.backup.damaged_refused` | covered | `crates/nvmod-cli/tests/backup.rs::a_damaged_snapshot_is_refused_with_exit_3_and_nothing_changes` |
| A failed restore puts the Profile back | `safety.backup.failed_restore_puts_back` | covered | `crates/nvmod-cli/tests/backup.rs::a_failed_restore_puts_the_profile_back_and_exits_4` |
| A restore is refused while a run of the Instance is alive | `safety.backup.refused_while_running` | covered | `crates/nvmod-cli/tests/backup.rs::restore_refuses_while_a_run_of_the_instance_is_alive_and_ignores_other_games` |
| An automatic snapshot is taken when the last one is older than the interval | `safety.backup.automatic_snapshot` | covered | `crates/nvmod-cli/tests/backup.rs::the_automatic_snapshot_is_taken_when_the_last_one_is_older_than_the_interval` |
| prune keeps the newest and one per day | `safety.backup.retention` | covered | `crates/nvmod-backup/src/retention.rs::forty_snapshots_keep_the_newest_five_and_one_per_day_for_fourteen_days`, `crates/nvmod-cli/tests/backup.rs::prune_keeps_the_newest_and_one_per_day` |
| Mod Organizer's own timestamped backups are listed and restorable | `safety.backup.mo2_backups` | covered | `crates/nvmod-cli/tests/backup.rs::mo2s_own_timestamped_backups_are_listed_and_restorable` |

### Importing from Mod Organizer 2 (`safety.import`)

| Item | Id | State | Tests |
| --- | --- | --- | --- |
| An MO2 instance imports with the same Mods and load order | `safety.import.same_mods_same_order` | covered | `crates/nvmod-cli/tests/import_mo2.rs::the_example_instance_imports_with_the_same_mods_and_load_order` |
| What cannot be mapped is listed | `safety.import.unmapped_listed` | covered | `crates/nvmod-cli/tests/import_mo2.rs::what_cannot_be_mapped_is_listed` |
| plugins.txt is decoded in the Windows code page | `safety.import.code_page` | covered | `crates/nvmod-cli/tests/import_mo2.rs::plugins_txt_is_decoded_in_the_windows_code_page` |
| Disagreeing plugin lists are reported per plugin and nothing is written | `safety.import.disagreement_refused` | covered | `crates/nvmod-cli/tests/import_mo2.rs::a_different_order_in_the_two_lists_is_reported_per_plugin`, `crates/nvmod-cli/tests/import_mo2.rs::a_plugin_missing_from_loadorder_is_one_disagreement_and_nothing_is_written` |
| Mods are hard linked unless copying is asked for | `safety.import.hard_links` | passing | `crates/nvmod-cli/tests/import_mo2.rs::mods_are_hard_linked_unless_copying_is_asked_for` |
| The imported Instance stages and passes the Doctor | `safety.import.imported_instance_works` | covered | `crates/nvmod-cli/tests/import_mo2.rs::the_imported_instance_stages_and_passes_the_doctor` |
| The Mod Organizer folder is never touched | `safety.import.mo2_untouched` | covered | `crates/nvmod-cli/tests/profile_ops.rs::an_imported_profile_can_be_copied_renamed_and_selected_and_mo2_is_untouched` |

### Saves (`safety.saves`)

| Item | Id | State | Tests |
| --- | --- | --- | --- |
| List saves with their plugin counts; unreadable ones are listed as such | `safety.saves.list` | covered | `crates/nvmod-cli/tests/saves.rs::list_shows_saves_with_plugin_counts_and_unreadable_ones` |
| Back up, delete and restore round-trips | `safety.saves.backup_restore` | covered | `crates/nvmod-cli/tests/saves.rs::backup_delete_restore_round_trips_through_the_commands`, `crates/nvmod-saves/tests/saves.rs::backup_then_delete_then_restore_is_byte_identical` |
| A restore backs up what is there and never deletes | `safety.saves.restore_never_deletes` | covered | `crates/nvmod-saves/tests/saves.rs::restore_backs_up_what_is_there_and_never_deletes` |
| Transfer saves with their cosaves between Profiles | `safety.saves.transfer` | covered | `crates/nvmod-cli/tests/saves.rs::transfer_refuses_without_the_cosave_and_force_copies` |
| check reports a save that needs a plugin the Profile lacks | `safety.saves.check_missing_plugin` | covered | `crates/nvmod-cli/tests/saves.rs::check_exits_1_for_a_save_that_needs_a_missing_plugin` |
| checksum lists the hashes of the saves | `safety.saves.checksums` | covered | `crates/nvmod-cli/tests/saves.rs::checksum_lists_hashes_and_a_shared_profile_needs_a_prefix` |
| The save folder follows the LocalSaves switch | `safety.saves.local_saves_folder` | covered | `crates/nvmod-saves/tests/saves.rs::the_save_folder_follows_local_saves` |

### Undo and interruption (`safety.undo`)

| Item | Id | State | Tests |
| --- | --- | --- | --- |
| A journal in the old format is still rolled back | `safety.undo.old_journal` | covered | `crates/nvmod-journal/tests/old_journal.rs::an_install_journal_in_the_old_format_is_rolled_back` |
| rollback restores a schema 1 file byte for byte | `safety.undo.schema_rollback` | covered | `crates/nvmod-cli/tests/instance_schema.rs::rollback_restores_the_schema_1_file_byte_for_byte` |
| SIGTERM during an install rolls the step back | `safety.undo.sigterm_install` | passing | `crates/nvmod-cli/tests/sigterm.rs::sigterm_during_install_rolls_the_step_back_and_exits_4` |
| SIGTERM during a Mods change or a sort restores the Profile files | `safety.undo.sigterm_lists` | passing | `crates/nvmod-cli/tests/sigterm.rs::sigterm_during_mods_and_sort_apply_restores_the_profile_files` |
| SIGTERM during an import removes the new Instance | `safety.undo.sigterm_import` | passing | `crates/nvmod-cli/tests/sigterm.rs::sigterm_during_import_mo2_removes_the_new_instance` |

### Conflicts (`staging.conflicts`)

| Item | Id | State | Tests |
| --- | --- | --- | --- |
| Each contested path lists its winner and its losers in order | `staging.conflicts.winners_and_losers` | covered | `crates/nvmod-cli/tests/conflicts.rs::five_collisions_are_listed_once_each_with_their_winner_and_ordered_losers` |
| Reordering changes the winner | `staging.conflicts.reorder_changes_winner` | covered | `crates/nvmod-cli/tests/conflicts.rs::reordering_changes_the_winner` |
| A Mod that loses every file is redundant | `staging.conflicts.redundant_mods` | covered | `crates/nvmod-cli/tests/conflicts.rs::a_mod_that_loses_every_file_is_redundant_and_one_that_wins_some_is_mixed` |
| A generated Mod is listed as generated and left out of the conflicts | `staging.conflicts.generated_left_out` | covered | `crates/nvmod-cli/tests/conflicts.rs::a_generated_mod_is_listed_as_generated_and_left_out_of_the_conflicts_with_one_reason` |
| One Mod's own row and the paths it is in | `staging.conflicts.per_mod_view` | covered | `crates/nvmod-api/tests/conflicts.rs::a_named_mod_keeps_its_own_row_and_the_paths_it_is_in` |
| Loose files over archives are reported when asked for | `staging.conflicts.archive_axes` | covered | `crates/nvmod-cli/tests/archives.rs::archives_lists_the_loose_and_archive_losers_with_provider_names` |
| An archive no ini lists is reported | `staging.conflicts.archive_not_listed` | covered | `crates/nvmod-cli/tests/archives.rs::archive_not_listed_has_a_positive_and_a_negative_fixture` |
| An archive an ini lists but that is missing is reported | `staging.conflicts.archive_listed_missing` | covered | `crates/nvmod-cli/tests/archives.rs::archive_listed_missing_has_a_positive_and_a_negative_fixture` |
| Archive invalidation problems are reported | `staging.conflicts.archive_invalidation` | covered | `crates/nvmod-cli/tests/archives.rs::archive_invalidation_has_a_positive_and_a_negative_fixture` |
| A corrupt archive header is reported | `staging.conflicts.archive_corrupt_header` | covered | `crates/nvmod-cli/tests/archives.rs::archive_corrupt_header_has_a_positive_and_a_negative_fixture` |

### Sweeping what a program wrote (`staging.sweep`)

| Item | Id | State | Tests |
| --- | --- | --- | --- |
| A file a program created goes to Overwrite with its folders | `staging.sweep.to_overwrite` | covered | `crates/nvmod-cli/tests/sweep.rs::a_created_file_goes_to_overwrite_with_its_folders_and_the_tree_shows_it_from_there` |
| A replaced file keeps the Mod intact and its new content lands in Overwrite | `staging.sweep.replaced_file` | covered | `crates/nvmod-cli/tests/sweep.rs::a_replaced_file_keeps_the_mod_intact_and_its_new_content_lands_in_overwrite` |
| A deleted file is restored from its provider and reported | `staging.sweep.deleted_restored` | covered | `crates/nvmod-cli/tests/sweep.rs::a_deleted_file_is_restored_from_the_provider_and_reported` |
| A protected file written in place is staged as a copy from then on | `staging.sweep.protect` | covered | `crates/nvmod-cli/tests/sweep.rs::a_file_written_in_place_is_reported_and_protect_stages_it_as_a_copy_from_then_on` |
| A custom target takes the output instead of Overwrite | `staging.sweep.custom_target` | covered | `crates/nvmod-cli/tests/sweep.rs::a_custom_target_takes_the_output_instead_of_overwrite` |
| An interrupted sweep gives the same tree when it runs again | `staging.sweep.repeatable` | covered | `crates/nvmod-cli/tests/sweep.rs::an_interrupted_sweep_gives_the_same_final_tree_when_it_runs_again` |
| A sweep is never rolled back, so what a program wrote is never lost | `staging.sweep.never_rolled_back` | covered | `crates/nvmod-cli/tests/sweep.rs::a_sweep_is_not_rolled_back_so_what_a_program_wrote_is_never_lost` |

### The Staging tree (`staging.tree`)

| Item | Id | State | Tests |
| --- | --- | --- | --- |
| Every path of the tree is lower case and holds the game's files and the Mods' | `staging.tree.lowercase_paths` | covered | `crates/nvmod-cli/tests/stage.rs::every_path_of_the_staging_tree_is_lowercase_and_holds_game_files_and_mods` |
| Where two Mods provide a file the later one in the load order wins | `staging.tree.later_mod_wins` | covered | `crates/nvmod-cli/tests/stage.rs::where_two_mods_provide_a_file_the_later_mod_in_the_load_order_wins` |
| A generated Mod (the far view) wins over every other Mod | `staging.tree.generated_mod_wins` | covered | `crates/nvmod-cli/tests/stage.rs::a_generated_mod_wins_over_every_other_mod_wherever_it_is_listed` |
| The game's executable never enters the tree | `staging.tree.game_executable_excluded` | covered | `crates/nvmod-cli/tests/stage.rs::the_game_executable_never_enters_the_staging_tree` |
| Files are hard linked where the file system allows | `staging.tree.hard_links` | passing | `crates/nvmod-cli/tests/stage.rs::files_are_hard_linked_where_the_filesystem_allows` |
| Where links cannot be made the files are copied, and the Doctor says so | `staging.tree.copy_fallback` | covered | `crates/nvmod-cli/tests/stage.rs::where_links_cannot_be_made_the_files_are_copied_and_the_doctor_reports_it` |
| A change to one Mod rebuilds about its files, not the whole tree | `staging.tree.incremental` | covered | `crates/nvmod-cli/tests/stage_incremental.rs::a_changed_winner_replaces_one_file_and_matches_a_full_build`, `crates/nvmod-cli/tests/stage_incremental.rs::enabling_one_mod_changes_about_its_files_not_the_whole_tree` |
| A killed run is undone by the next one; the old tree stays whole until the swap | `staging.tree.crash_safe` | covered | `crates/nvmod-cli/tests/stage.rs::a_failed_step_exits_four_and_leaves_the_old_tree_in_place`, `crates/nvmod-cli/tests/stage_incremental.rs::a_killed_run_is_undone_by_the_next_run_which_then_gives_the_new_tree` |
| tree lists every path once with the layer that provides it | `staging.tree.tree_listing` | covered | `crates/nvmod-api/tests/tree.rs::every_path_is_listed_once_with_the_layer_that_provides_it` |
| A hidden file is not a provider | `staging.tree.hidden_files` | covered | `crates/nvmod-api/tests/tree.rs::a_hidden_file_is_not_a_provider_and_the_next_layer_wins` |
| Overwrite sits above every Mod | `staging.tree.overwrite_on_top` | covered | `crates/nvmod-api/tests/tree.rs::an_overwrite_folder_sits_above_every_mod` |
| Paths are looked up ignoring case and slash style | `staging.tree.case_insensitive_index` | covered | `crates/nvmod-core/src/index.rs::lookup_ignores_case_and_slash_style` |
| A tree without one Mod shows what the next provider gives | `staging.tree.tree_without_a_mod` | covered | `crates/nvmod-stage/tests/stage_without.rs::a_tree_without_a_mod_has_the_next_providers_files_and_not_its_plugin` |

### Contracts with the other Components (`tools.contracts`)

| Item | Id | State | Tests |
| --- | --- | --- | --- |
| contracts lists what this nvmod accepts | `tools.contracts.contracts_listed` | passing | `crates/nvmod-cli/tests/tools.rs::contracts_lists_what_this_nvmod_accepts` |
| Every contract accepts its recorded sample | `tools.contracts.recorded_samples` | covered | `crates/nvmod-tools/tests/contracts.rs::every_contract_accepts_its_recorded_sample` |
| A newer schema version is refused with a reason | `tools.contracts.newer_refused` | covered | `crates/nvmod-tools/tests/contracts.rs::a_newer_schema_version_is_refused_with_a_reason` |
| A missing required field is rejected with the contract named | `tools.contracts.required_field` | covered | `crates/nvmod-tools/tests/contracts.rs::a_missing_required_field_is_rejected_with_the_contract_named` |
| Unknown fields are ignored | `tools.contracts.unknown_fields_ignored` | covered | `crates/nvmod-tools/tests/contracts.rs::unknown_fields_are_ignored` |
| The highest common version not above the lock is picked | `tools.contracts.version_negotiation` | covered | `crates/nvmod-tools/src/negotiate.rs::the_highest_common_version_not_above_the_lock_is_picked` |
| The library result and the sort JSON output are equal and match the schema | `tools.contracts.loot_sort_contract` | covered | `crates/nvmod-tools/tests/loot_sort.rs::the_library_result_and_the_sort_json_output_are_equal`, `crates/nvmod-tools/tests/loot_sort.rs::the_sort_json_output_matches_the_vendored_schema` |
| Every schema of the lodwright handshake is vendored at version 1 | `tools.contracts.lodwright_vendored` | covered | `crates/nvmod-tools/tests/contracts.rs::every_schema_of_the_lodwright_handshake_is_vendored_at_version_1` |
| The engine's release manifest shape validates | `tools.contracts.nv_rs_manifest` | covered | `crates/nvmod-tools/tests/contracts.rs::the_forks_real_manifest_shape_validates` |
| The capabilities file is read; a broken one is refused with a reason | `tools.contracts.capabilities_file` | covered | `crates/nvmod-tools/src/capabilities.rs::a_file_that_breaks_the_contract_is_refused_with_a_reason`, `crates/nvmod-tools/src/capabilities.rs::counts_are_read_and_missing_is_derived` |

### Pinned tools (`tools.pins`)

| Item | Id | State | Tests |
| --- | --- | --- | --- |
| A pinned tool is fetched, checksum and attestation verified, and stored by hash | `tools.pins.fetch_verified` | passing | `crates/nvmod-cli/tests/tools.rs::a_pinned_tool_is_fetched_verified_attested_and_stored_by_hash` |
| A checksum mismatch installs nothing | `tools.pins.checksum_mismatch` | passing | `crates/nvmod-cli/tests/tools.rs::a_checksum_mismatch_exits_3_installs_nothing_and_never_asks_for_the_attestation` |
| A failed attestation leaves no file behind | `tools.pins.attestation_failure` | passing | `crates/nvmod-cli/tests/tools.rs::a_failed_attestation_exits_3_and_leaves_no_file_behind` |
| A network failure is retried three times, a malformed response is not | `tools.pins.retries` | passing | `crates/nvmod-cli/tests/tools.rs::a_malformed_response_is_not_retried_and_a_network_failure_is_retried_three_times` |
| A pin bump never overwrites the tool in use | `tools.pins.pin_bump_safe` | passing | `crates/nvmod-cli/tests/tools.rs::a_pin_bump_never_overwrites_the_tool_in_use` |
| A third-party tool without a pinned checksum is refused until the owner trusts it | `tools.pins.third_party_trust` | passing | `crates/nvmod-cli/tests/tools.rs::a_third_party_installer_without_a_pinned_checksum_is_refused_until_the_owner_trusts_it` |
| A lock that breaks the rules is refused with the reason | `tools.pins.lock_rules` | passing | `crates/nvmod-cli/tests/tools.rs::a_lock_that_breaks_the_rules_is_refused_with_the_reason` |
| loot-rs is linked, and the lock records the tag Cargo.toml depends on | `tools.pins.linked_loot_rs` | covered | `crates/nvmod-cli/tests/tools.rs::the_shipped_lock_lists_loot_rs_as_linked`, `crates/nvmod-tools/src/lock.rs::the_shipped_lock_is_valid_and_records_the_loot_rs_tag_of_cargo_toml` |
| The runner rechecks the checksum on every call | `tools.pins.runner_rechecks` | passing | `crates/nvmod-cli/tests/tools.rs::the_runner_rechecks_the_checksum_on_every_call_and_exits_3_when_the_binary_changed` |
| list and verify need no network | `tools.pins.offline_verify` | passing | `crates/nvmod-cli/tests/tools.rs::list_shows_each_tool_with_its_pinned_version_and_state_and_verify_needs_no_network` |
| SHA-256 digests are correct | `tools.pins.sha256` | covered | `crates/nvmod-runner/src/sha256.rs::known_digests` |

### Running external tools safely (`tools.runner`)

| Item | Id | State | Tests |
| --- | --- | --- | --- |
| Arguments are passed as words with no shell | `tools.runner.no_shell` | passing | `crates/nvmod-runner/src/runner.rs::arguments_are_passed_as_words_with_no_shell` |
| The environment is scrubbed | `tools.runner.scrubbed_environment` | passing | `crates/nvmod-runner/src/runner.rs::the_environment_is_scrubbed_and_extras_are_passed` |
| A tool that runs too long or writes too much is killed | `tools.runner.time_and_output_limits` | passing | `crates/nvmod-runner/src/runner.rs::a_tool_that_runs_too_long_is_killed_and_reported_as_failed`, `crates/nvmod-runner/src/runner.rs::a_tool_that_writes_past_the_cap_is_killed` |
| A failing tool is a tool failure with the first line of its error | `tools.runner.failure_reported` | passing | `crates/nvmod-runner/src/runner.rs::a_failing_exit_code_is_a_tool_failure_with_the_first_line_of_its_error` |
| Only the runner crate starts processes | `tools.runner.only_runner_spawns` | covered | `crates/nvmod-runner/tests/no_command_elsewhere.rs::only_the_runner_crate_uses_process_command` |
