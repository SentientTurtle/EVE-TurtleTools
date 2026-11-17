# TurtleTools alternative Icon set for EVE Online

Alternative set of inventory icons for EVE Online, containing various fixes and additions.

The icon set is provided in multiple formats by automated GH Actions release builds. Below is documentation for each format, with additional information in fold-out 'Details' boxes.

## Styles

Several 'styles' of the icon set are provided. Additional configurations may be built with the CLI icon generator tool.

* No-suffix files ("Image Export Collection.zip", "Service Bundle.zip") contain standard game-accurate icons.  
* "Old Style" suffixed files contain icons with the old "glossy" style overlays for tech-tier.  
* "Bonus Style" suffixed files contain icons with additional alpha/omega & module slot overlays.
* "Clean" suffixed files contain icons with no overlays applied, with metadata file `overlay_metadata.json` and overlays.
  <details>
    <summary>Clean Style usage Details</summary>
    The clean icon sets ("Image Export Collection - Clean.zip", "Service Bundle - Clean.zip") contain the icon files in the same format as their no-suffix equivalents, but have the following additions:  
  
    * Overlays in the `/overlay/` folder, with a subfolder `/new/` for the current style of overlays, and `/old/` for the old "shiny" style of overlays
    * A `overlay_metadata.json` file describing which overlays apply to which typeID:
      ```json
      {
        // Mapping of { typeID: [overlay] }, matching the filenames in the `/overlay/{style}/` folder
        "405": [
          "meta-storyline",
          "module-high",
          "clone-alpha"
        ],
        "406": [
          "module-high",
          "meta-t2",
          "clone-alpha"
        ],
        ...
      }
      ```
    Overlays should be enabled by allow-list to enable only the desired overlays, and additions of new kinds of overlay should be accepted as a NON-BREAKING change. Some overlays are "NON-CANONICAL" and do not show up in the game, but are provided as optional additions. The following overlays are currently included:
    
    * `clone-alpha` \[NON-CANONICAL]: The item-type requires skills to use and can be used by an Alpha (Free to Play) character.
    * `clone-omega` \[NON-CANONICAL]: The item-type requires skills to use and can be only be used by an Omega (Pay to Play) character. In-game this displays as a yellow-gold border around the type icon.
    * `meta-t1` \[NON-CANONICAL]: Tech Tier 1 MetaGroup (#1). Not displayed ingame, nor specified in the `overlay_metadata.json` file. Applicable to any type with either no metagroup or metagroup 1. (i.e. any type without a `meta-` overlay specified in the json file)
    * `meta-t2`: Tech Tier 2 MetaGroup (#2)
    * `meta-storyline`: Storyline MetaGroup (#3)
    * `meta-faction`: Faction MetaGroup (#4)
    * `meta-officer`: Officer MetaGroup (#5)
    * `meta-deadspace`: Deadspace MetaGroup (#6)
    * `meta-t3`: Tech Tier 3 MetaGroup (#14)
    * `meta-abyssal`: Abyssal MetaGroup (#15)
    * `meta-nes`: NES (Cash Shop) MetaGroup (#17)
    * `meta-timelimited`: TimeLimited MetaGroup (#19)
    * `meta-structure-faction`: Structure Faction MetaGroup (#52)
    * `meta-structure-t2`: Structure Tech Tier 2 MetaGroup (#53)
    * `meta-structure-t1`: Structure Tech Tier 1 MetaGroup (#54) Displayed in-game, unlike `meta-t1`.
    * `meta-academy`: Academy MetaGroup (#62)
    * `meta-aberrant`: Aberrant MetaGroup (#63)
    * `module-high`: Module uses High Power Slot
    * `module-medium`: Module uses Medium Power Slot
    * `module-low`: Module uses Low Power Slot
    * `module-rig`: Module uses Rig Slot
    * `module-subsystem`: Module uses Subsystem slot
    * \+ Future additions. Overlays are namespaced by the word before the first hyphen to facilitate enabling groups.
  </details>

## "Image Export Collection"

Simple icon-dump with files for each item-type, suitable for direct web hosting. Contains duplicate files.

If you apply additional processing or dynamically serve images, the 'Service Bundle' may be more convenient.

<details>

Naming convention follows that of the (now-deprecated) official Image-Export-Collection:
* For regular types, including blueprints/reactions/relics, "[type_id]_64.png";
* For blueprints, the blueprint-copy icon is available as "[type_id]_64_bpc.png"
  e.g.
* "648_64.png" for the 'Badger' ship (typeID #648)
* "983_64.png" for the 'Badger Blueprint' (typeID #983)
* "983_64_bpc.png" for the 'Badger Blueprint' blueprint-copy icon (typeID #983)

* For renders "[type_id]_512.jpg";
  e.g.
  * "648_512.jpg" for the 'Badger' ship (typeID #648)

Notes:
* Not all types have an icon or render
* Icons are all in PNG format, with a 64x64 px resolution
* Renders are all in JPG format, with a 512x512 px resolution, directly from the game files.
* Relics/Reactions do not have a 'blueprint copy' icon
</details>

## "Service bundle"

De-duplicated set of icons and metadata for using them.

If you require a folder of images that can directly be hosted as-is, use the 'Image Export Collection' compatible dump.

<details>

Metadata is contained in the `service_metadata.json` file, with the following structure:
```json
{
  // TypeID mapped to the available images
  "575": {
    // Most (but not all) types have a common inventory icon. A 64x64 pixel PNG file.
    "icon": "CB16E451689F9508404060893887F71F.png"
  },
  "648": {
    "icon": "77A112E2EB714AB54C1249306EF257C5.png",
    // Certain types such as ships have 'renders'; Larger 512x512 pixel images, provided in JPG format.
    "render": "AD4A5F0E2F1FAD53ADF38B8AF7BB0FE9.jpg"
  },
  ...
  "11292": {
      // Blueprint/Relic/Reaction types have both an 'icon' field and a 'bp' field, both pointing at the blueprint "original" icon
      "icon": "E90D987E20CB4BC00172FA6A1A2DF8AD.png",
      "bp": "E90D987E20CB4BC00172FA6A1A2DF8AD.png",
      // Blueprints have a 'bpc' field for blueprint copies, pointing at the blueprint "copy" icon
      "bpc": "41F5172C5523DA82BE25E500ABA01996.png"
    },
    ...
    "11294": {
      // Different entries point to the same file if they share an icon
      "icon": "E90D987E20CB4BC00172FA6A1A2DF8AD.png",
      "bp": "E90D987E20CB4BC00172FA6A1A2DF8AD.png",
      "bpc": "41F5172C5523DA82BE25E500ABA01996.png"
    },
}
```

### Example: A drop-in substitute for the official image service

Using the metadata a drop-in replacement for the official Image Service can be created. To do so, the following web routes must be created:

`/types/{typeID}` -> return the *keyset* of the matching entry in the service_metadata.json  
    - e.g.: `/type/648/` -> `["icon", "render"]`  
`/types/{typeID}/{variant}` -> return the associated image  
- e.g.: `/type/648/icon` -> `77A112E2EB714AB54C1249306EF257C5.png`

For full compatibility with the Fenris Creations' official Image Service the non-item routes need to be redirected to the official service:
* `/alliances/`
* `/characters/`
* `/corporations/`
</details>

## "Ship Tree Export"

Side-on Ship renders as displayed in the game's Ship Tree. Provided as semi-transparent PNG files ready for direct use or compositing.  
Files are provided in format `[TYPE_ID].png`, only for player-flyable ships.

## "Utility Icons"

Various UI and other icons previously included in the official Fenris Creations "Image Export Collection".  
Subject to change: Additional icons may be added. (For requests: Open an issue or direct-message)

## "PATCH" Files

Files ending in `-PATCH-{GH TAG}.zip` are patch files, containing only the changed files since the previous release and a `DELETED.txt` file listing the removed files, separated by newline.

Patch files can be applied by extracting the zip file over the previous release's, and optionally deleting specified files.  
E.g. Unzipping `Image Export Collection.zip` from release `icons-3693310`, then unzipping `Image Export Collection-PATCH-icons-3693310.zip` into the same directory & overriding existing files from the next release (`icons-3698585`) is equivalent to downloading `Image Export Collection.zip` from release `icons-3698585`.

# Changes

## Versions after `3693310`
* Added patch files for the remaining files

## Versions after `3670638`:
* Added `Service Bundle - Clean`; Contains icons without overlays and overlay metadata file, such that overlays may be applied by users at application runtime.
* Added 'Service Bundle-PATCH-{PREV_VERSION}' & 'Image Export Collection-PATCH-{PREV_VERSION}' files

## Versions after `3561556`:
* Added 'Utility Icons'

## Versions after `3484357`:
* Ship Tree Export now uses 'Grayscale with Alpha Channel' PNG encoding to reduce file sizes. This encoding yields identical image data as before. If you encounter errors, open an issue.