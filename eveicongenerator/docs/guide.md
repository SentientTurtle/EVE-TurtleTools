# EVE Icon Generator guide

### Global options
* `--user_agent <user_agent>`, `-u <user_agent>` *REQUIRED either 'user_agent' or 'user_agent_file'*  
  User agent for HTTP requests  
* `--user_agent_file <file>` *REQUIRED either 'user_agent' or 'user_agent_file'*  
  User agent for HTTP requests, stored in a file.  
* `--cache_folder <directory>`, `-c <directory>` (default: `./cache`)  
  Folder for game file cache.  
  WARNING: All other (unrelated) files in this folder will be deleted during clean-up.  
  This folder should persist between runs to avoid re-downloading files from FC servers.
* `--icon_folder <directory>`, `-i <directory>` (default: `./icons`)  
  Folder for storing built icons.
  This folder may be persisted to cache image-compositing work.
  NOTE: Other files in this folder will not be deleted. Only unnecessary files created by the previous run of this program will be cleaned up.
* `--logfile <file>`, `-l <file>` 
  Log file destination, if unset no logging is performed.  
  Log contains detailed information on icon generation & written output, and is several megabytes of text.
* `--append_log`
  If set, appends to the specified logfile. If omitted, truncates log file. Requires `--logfile`.
* `--silent`  
  Silent mode, implied by `checksum` output mode if no checksum file is specified.
* `--force_rebuild`, `-f`
  Force rebuilding of images, re-doing compositing of all icons. Recommended when updating the application to ensure any changes to compositing have been applied to cached icons.
* `--skip_if_fresh`, `-s`
  If no icons have changed since the last run, skip generating output.
  NOTE: Ignored for `checksum` output mode with no checksum file specified, the checksum will still be output to stdout.
* `old_overlays`
  Use old 'glossy' tech tier overlays
* `module_overlays`
  Add overlays for fitting slot requirements (as used in the in-game market)
* `clone_overlays`
  (CUSTOM CONTENT) Add overlays for Alpha/Omega clone requirements
* `no_purge`
  Do not purge icon cache folder, will still clean up 'SharedCache' `cache_folder`. (Intended for use with GH actions, should not be used when caching to a local hard disk or other persistent storage.)
* `image_format <format>`
  Convert icon images to specified format, availability of options depends on enabled features during compilation. "native" yields images as they are provided in the game files; A mix of PNG and JPEG.

Output mode subcommands:
* `help [subcommand]` Displays help text for the specified subcommand
* `service_bundle`
  Generates a de-duplicated icon .zip archive, including metadata compatible with the "Image Service" routes.
  * `--out <file>` Output file for zip archive, required.
* `iec`
  Generates an 'Image Export Collection'-compatible icon .zip archive.
  * `--out <file>` Output file for zip archive, required.
* `web_dir`  
  Prepares a directory for web hosting 'image service' compatible routes by creating symlinks & metadata files, see "webmode.md".
  * `--out <directory>` Output directory to write into, required.
  * `--copy_files` Copies files rather than using symlinks.
  * `--hardlink` Use hard links rather than using soft links.
* `utility_icons`
  Generates an 'Image Export Collection'-compatible icon .zip archive.
  * `--out <file>` Output file for zip archive, required.
  * `--listfile <file>` Override default list of utility icons.
* `checksum`
  Emits a checksum of the current icon index, writes to stdout if no output file is specified.
  * `--out <file>` Output file for checksum, optional.
* `shiptree`
  Export of ship tree ship renders, in filename format `{typeID}.png`.
  * `--out <file>` Output file for zip archive, required.
* `aux_icon`
  Auxiliary Icon export, builds .zip archive with all "iconID" icons, in filename format `{iconID}.png`/`{iconID}.jpg`.
  * `--out <file>` Output file for zip archive, required.
* `aux_all`
  Auxiliary all-image export, builds .zip archive with all images in the game cache.
  * `--out <file>` Output file for zip archive, required.
  * `--incl-character` Include character model texture images. This adds several gigabytes of data to the export AND cache folder. (~1GB -> ~6GB, 2x totalling ~12GB of storage needed)
* `multi`
  Multi-output mode. Updates image data once, then generates multiple outputs with that version.  
  If checksum-to-stdout is chosen, silent mode is enabled for all outputs and only a checksum will be emitted to stdout upon completion of all outputs.
  * `--service_bundle <file>` Enable service bundle output.
  * `--iec <file>` Enable 'Image Export Collection' output.
  * `--web_dir <directory>` Enable web directory output, allows additional options for config.
    * `--copy_files` Copies files rather than using symlinks.
    * `--hardlink` Use hard links rather than using soft links.
  * `--utility-icons <file>` Generates an 'Image Export Collection'-compatible icon .zip archive.
    * `--utility_listfile <file>` Override default list of utility icons.
  * `--shiptree <file>` Export of ship tree ship renders, in filename format `{typeID}.png`.
  * `--aux_icons <file>` Enable Auxiliary Icon output.
  * `--aux_all <file>` Enable Auxiliary all-image output.
    * `--incl-character` Include character model texture images. This adds several gigabytes of data to the export AND cache folder. (~1GB -> ~6GB, 2x totalling ~12GB of storage needed)