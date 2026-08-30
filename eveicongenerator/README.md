## Icon "Export Collection"

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

## Icon "Service bundle"

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

## Styles

* No-suffix files ("Image Export Collection.zip", "Service Bundle.zip") contain standard game-accurate icons.  
* "Old Style" suffixed files contain icons with the old "glossy" style overlays for tech-tier.  
* "Bonus Style" suffixed files contain icons with additional alpha/omega & module slot overlays.

## Ship Tree Export

Side-on Ship renders as displayed in the game's Ship Tree. Provided as semi-transparent PNG files ready for direct use or compositing.
Files are provided in format `[TYPE_ID].png`, only for player-flyable ships.

# Changes

## Versions after `3484357`:
* Ship Tree Export now uses 'Grayscale with Alpha Channel' PNG encoding to reduce file sizes. This encoding yields identical image data as before. If you encounter errors, open an issue.