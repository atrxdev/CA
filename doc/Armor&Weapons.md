In **Red Alert 2 / Yuri's Revenge**, armor and damage are handled through a **Warhead vs Armor** system. Every unit has an armor type, and every weapon has a warhead with damage multipliers ("Verses") against each armor type. The actual damage dealt is:

**Final Damage = Weapon Damage × Warhead Verses Modifier** 

Here is a comprehensive breakdown of how armor and damage function in the game.

## Armor Types

The original game uses **11 armor types**. Every unit and structure in the game is assigned one of **11 specific armor types**. The armor determines how vulnerable a unit is to different weapons.  

| **Category**       | **Armor Type** | **Description & Notable Examples**                           |
| ------------------ | -------------- | ------------------------------------------------------------ |
| **Infantry**       | `None`         | Unarmored units. (e.g., Conscripts, GIs, Attack Dogs, Engineers) |
|                    | `Flak`         | Lightly armored soldiers. (e.g., Flak Troopers, Rocketeers, Tanya) |
|                    | `Plate`        | Heavily armored infantry and cyborgs. (e.g., Tesla Troopers, Chrono Legionnaires) |
| **Vehicles & Air** | `Light`        | Fast or lightly plated units and aircraft. (e.g., IFVs, Harriers, Dolphins, V3 Launchers) |
|                    | `Medium`       | Standard armored vehicles and transports. (e.g., Grizzly Tanks, Amphibious Transports) |
|                    | `Heavy`        | Massive, durable vehicles and capital ships. (e.g., Apocalypse Tanks, Rhino Tanks, MCVs, Dreadnoughts) |
| **Structures**     | `Wood`         | Basic or standard buildings. (e.g., Power Plants, Barracks, War Factories) |
|                    | `Steel`        | Base defenses and fortified outposts. (e.g., Pillboxes, Prism Towers, Tesla Coils, Patriot Missiles) |
|                    | `Concrete`     | Massive, critical infrastructure. (e.g., Construction Yards, Nuclear Reactors, Superweapons) |
| **Special**        | `Special_1`    | A unique armor class created specifically for **Terror Drones** to handle their unique mechanical/infantry vulnerabilities. |
|                    | `Special_2`    | Used for **destructible projectiles** (e.g., V3 Rockets, Dreadnought Missiles) to prevent splash damage from instantly destroying allied missiles mid-air. |



## Common Warhead Types

The game defines many warheads, but most weapons fall into these categories:

| Warhead Type        | Used By                             | Strong Against       |
| ------------------- | ----------------------------------- | -------------------- |
| SA (Small Arms)     | GI rifle, Conscript rifle           | Infantry             |
| AP (Armor Piercing) | Tank cannons                        | Vehicles             |
| HE (High Explosive) | Artillery, V3, grenades             | Infantry, buildings  |
| Fire                | Desolator effects, flames           | Infantry, structures |
| Electric            | Tesla weapons                       | Vehicles, infantry   |
| Hollow Point        | Sniper weapons                      | Infantry only        |
| Super               | Chrono Legionnaire, special weapons | Everything           |

## Typical Damage Multipliers

Some of the most important relationships are:

### Small Arms (Machine Guns)

| Armor          | Damage |
| -------------- | ------ |
| None           | 100%   |
| Flak           | 100%   |
| Plate          | 100%   |
| Light Vehicle  | ~2%    |
| Medium Vehicle | ~2%    |
| Heavy Vehicle  | ~2%    |
| Buildings      | ~2%    |

Machine guns shred infantry but are nearly useless against vehicles. 

### Armor Piercing (Tank Cannons)

| Armor  | Damage  |
| ------ | ------- |
| None   | ~25%    |
| Flak   | ~50%    |
| Plate  | ~75%    |
| Light  | 100%    |
| Medium | ~40-50% |
| Heavy  | 100%    |

This is why tank destroyers are excellent against tanks but poor against infantry. 

### High Explosive

| Armor     | Damage |
| --------- | ------ |
| None      | 150%   |
| Flak      | 100%   |
| Plate     | 50%    |
| Light     | 60%    |
| Medium    | 10%    |
| Heavy     | 10%    |
| Buildings | 20-30% |

Excellent against infantry, poor against heavy armor. 

### Sniper (Hollow Point)

| Armor     | Damage |
| --------- | ------ |
| None      | 200%   |
| Flak      | 100%   |
| Plate     | 100%   |
| Vehicles  | ~1%    |
| Buildings | ~1%    |

Used by Snipers and British Sniper IFVs. 

## Practical Counter System

The game's balance can be summarized as:

| Unit Type      | Best Counter                                   |
| -------------- | ---------------------------------------------- |
| Basic Infantry | Machine guns, explosives                       |
| Heavy Infantry | Machine guns, Tesla                            |
| Light Vehicles | Cannons, missiles                              |
| Heavy Tanks    | Tank destroyers, Apocalypse tanks, Prism tanks |
| Buildings      | Siege weapons, V3, Prism Tanks                 |
| Aircraft       | Flak, AA missiles                              |



### Special & Exotic Damage

Yuri's Revenge and the base game also feature a few exotic "weapons" that bypass the standard health-bar logic entirely:

- **Radiation:** Creates a localized toxic field that instantly liquefies infantry (`None`/`Flak`) and heavily degrades vehicle plating. `Plate` armor (like the Desolator himself) is immune.
- **Mind Control:** Does zero actual HP damage; instead, it flips the allegiance of the target. Useless against `Special_1` (Terror Drones), Attack Dogs, and all structures.
- **Temporal (Chrono):** Does not reduce hit points. Instead, it "erases" the unit from the timeline. The heavier the armor/HP of the unit, the longer it takes to erase them.
- **Parasitic:** Terror Drones bypass armor entirely by jumping inside a vehicle, applying a hardcoded constant health drain until the unit dies or is repaired at a Service Depot.