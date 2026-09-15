pub mod reflist;

pub mod user_agent {
    use std::ops::Deref;

    pub struct UserAgent(String);
    impl Deref for UserAgent {
        type Target = str;

        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }

    impl UserAgent {
        pub fn new(app_name: &str, app_version: &str) -> UABuilder {
            UABuilder {
                app_name: app_name.to_string(),
                app_version: app_version.to_string(),
                contacts: Vec::with_capacity(1),    // At least 1 contact is required
                comments: Vec::new(),
            }
        }
    }
    
    pub struct UABuilder {
        app_name: String,
        app_version: String,
        contacts: Vec<(String, String)>,
        comments: Vec<String>,
    }
    
    impl UABuilder {
        pub fn build(self) -> Result<UserAgent, &'static str> {
            if self.contacts.len() == 0 { return Err("Must have at least 1 contact") }

            use std::fmt::Write;
            let mut buf = String::new();
            write!(buf, "{}/{} (", self.app_name, self.app_version).expect("write into string");
            let mut first = true;
            for (kind, contact) in self.contacts {
                if first { first = false; } else { buf.push_str("; "); }
                write!(buf, "{}:{}", kind, contact).expect("write into string");
            }
            buf.push_str(") ");
            
            if self.comments.len() > 0 {
                first = false;
                buf.push('(');

                for comment in self.comments {
                    if first { first = false; } else { buf.push_str("; "); }
                    buf.push_str(&comment);
                }
                buf.push_str(") ");
            }
            
            write!(buf, "{}/{} (+{})", crate::CRATE_NAME, crate::CRATE_VERSION, crate::CRATE_REPO).expect("write into string");
            
            Ok(UserAgent(buf))
        }
    }
}
pub mod units {
    #[allow(non_camel_case_types)]
    #[repr(u32)]
    #[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
    #[cfg_attr(feature = "sde_load", derive(serde_repr::Serialize_repr, serde_repr::Deserialize_repr))]
    pub enum EVEUnit {
        Meter = 1,
        Kilogram = 2,
        Second = 3,
        Ampere = 4,
        Kelvin = 5,
        Mol = 6,
        Candela = 7,
        M2 = 8,
        M3 = 9,
        M_per_sec = 10,
        M_per_sec2 = 11,    // TODO This has display name 'm/sec' which is wrong?
        WaveNumber = 12,
        Kg_per_m3 = 13,
        M3_per_kg = 14,
        A_per_m2 = 15,
        A_per_m = 16,
        Mol_per_m3 = 17,
        Candela_per_m2 = 18,
        MassFraction = 19,
        Milliseconds = 101,
        Millimeters = 102,
        MegaPascals = 103,
        Multiplier = 104,
        Percentage = 105,
        Teraflops = 106,
        MegaWatts = 107,
        InversePercentage = 108,
        ModifierPercent = 109,
        InverseModifierPercent = 111,
        Rad_per_sec = 112,
        Hitpoints = 113,
        GigaJoule = 114,
        GroupID = 115,
        TypeID = 116,
        SizeClass = 117,
        OreUnits = 118,
        AttributeID = 119,
        Points = 120,
        RealPercent = 121,
        FittingSlots = 122,
        Seconds = 123,
        ModifierRelativePercent = 124,
        Newton = 125,
        LightYear = 126,
        AbsolutePercent = 127,
        Mbit_per_sec = 128,
        Hours = 129,
        ISK = 133,
        M3_per_Hour = 134,
        AU = 135,
        Slot = 136,
        Boolean = 137,
        Units = 138,
        Bonus = 139,
        Level = 140,
        Hardpoints = 141,
        Sex = 142,
        Datetime = 143,
        AU_per_Second = 144,
        ModifierRealPercent = 205,
    }   // TODO: Port formatter function from Java, don't forget non-breaking spaces!

    impl EVEUnit {
        pub fn unit_id(self) -> u32 {
            self as u32
        }

        pub fn format(self, _value: f64) -> String {
            unimplemented!()
        }
    }
}

