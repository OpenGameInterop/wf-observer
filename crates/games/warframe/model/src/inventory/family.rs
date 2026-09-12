use strum::VariantArray;

/// Semantic inventory families.
#[boltffi::data]
#[derive(Debug, Hash, VariantArray, ..Copy, ..Ord, ..Serde)]
#[non_exhaustive]
#[repr(u8)]
pub enum InventoryFamily {
    /// Primary Weapons — owned equipment for the Primary slot.
    LongGuns = 0,
    /// Secondary Weapons — owned equipment for the Secondary slot.
    Pistols = 1,
    /// Warframes — owned Warframe equipment.
    Warframes = 2,
    /// Melee Weapons — owned equipment for the Melee slot.
    Melee = 3,
    /// Weapon Skins — owned weapon appearance instances.
    WeaponSkins = 4,
    /// Internal raw-upgrade collection, not a decoded Mod representation.
    RawUpgrades = 5,
    /// Gear — owned consumable Gear items.
    Consumables = 6,
    /// Resources — stackable resources, components, and miscellaneous account items.
    MiscItems = 7,
    /// Sentinels — owned Sentinel companions.
    Sentinels = 8,
    /// Robotic Weapons — weapons equipped by Sentinels and MOAs.
    SentinelWeapons = 9,
    /// Beast Companions — owned Kubrows, Kavats, Predasites, and Vulpaphylas.
    KubrowPets = 10,
    /// Archwings — owned Archwing equipment.
    SpaceSuits = 11,
    /// Archguns — owned Archgun equipment.
    SpaceGuns = 12,
    /// Arch-Melee — owned Arch-Melee equipment.
    SpaceMelee = 13,
    /// Arcata — owned Lunaro equipment.
    Scoops = 14,
    /// Endo — transient reward bundles during missions before conversion into the Endo balance on extraction.
    FusionBundles = 15,
    /// Decorations — owned Orbiter and ship decorations.
    ShipDecorations = 16,
    /// Inbox rewards — items delivered through Inbox messages.
    EmailItems = 17,
    /// Argon Crystals — decay-tracking records for crystals found today.
    FoundToday = 18,
    /// Keys and Relics — owned mission keys and Void Relics.
    LevelKeys = 19,
    /// Blueprints — owned crafting recipes.
    Recipes = 20,
    /// Amps — owned Operator weapons, including Sirocco.
    OperatorAmps = 21,
    /// Exalted Weapons — ability-linked weapons and other special equipment.
    SpecialItems = 22,
    /// MOAs and Hounds — owned modular robotic companions.
    MoaPets = 23,
    /// K-Drives — owned hoverboard equipment.
    Hoverboards = 24,
    /// Railjacks — owned personal vessels.
    CrewShips = 25,
    /// Railjack Components and Armaments — owned built Railjack equipment.
    CrewShipWeapons = 26,
    /// Wreckage — recovered Railjack Components and Armaments awaiting repair or disposal.
    CrewShipSalvagedWeapons = 27,
    /// No confirmed player-facing category — reserved Drifter firearm equipment.
    DrifterGuns = 28,
    /// Drifter Melee Weapons — owned weapons available to the Drifter in Duviri.
    DrifterMelee = 29,
    /// No confirmed player-facing category — reserved gadget equipment.
    Gadgets = 30,
    /// Kaithes — owned Duviri mounts.
    Horses = 31,
    /// Railjack Resources — raw salvage and crafting resources.
    CrewShipRawSalvage = 32,
    /// Parazons — owned Parazon equipment.
    DataKnives = 33,
    /// Necramechs — owned Necramech equipment.
    MechSuits = 34,
    /// Plexus — personal Railjack Mod equipment.
    CrewShipHarnesses = 35,
    /// Atomicycles — owned 1999 motorcycles.
    Motorcycles = 36,
    /// Operator and Drifter Suits — owned Tenno apparel equipment.
    OperatorSuits = 37,
    /// Tektolyst Artifacts — owned Focus School weapons used for Tauron Strikes.
    Antiques = 38,
    /// Resources — mission-earned fish, ores, gems, and similar items before account merge.
    BonusMiscItems = 39,
}

impl InventoryFamily {
    /// Every family in schema order.
    pub const ALL: &'static [Self] = Self::VARIANTS;
}
