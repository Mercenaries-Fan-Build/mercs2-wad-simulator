import("WifEquipmentData")
import("MrxFactionManager")
import("MrxPmc")
import("MrxSupportData")
import("MrxUnlockFanfare")
import("WifMissionData")
import("WifMissionFlow")
import("MrxUtil")
_tCashReward = {
  none = 0,
  chapter_one_tiny = 5000,
  chapter_one_small = 100000,
  chapter_one_medium = 300000,
  chapter_one_large = 500000,
  chapter_one_boss = 750000,
  chapter_two_tiny = 50000,
  chapter_two_small = 500000,
  chapter_two_medium = 1000000,
  chapter_two_large = 2000000,
  chapter_two_boss = 2000000
}
_tFuelReward = {
  none = 0,
  chapter_one_tiny = 5,
  chapter_one_small = 100,
  chapter_one_medium = 250,
  chapter_one_large = 500,
  chapter_one_boss = 1000,
  chapter_two_tiny = 10,
  chapter_two_small = 200,
  chapter_two_medium = 500,
  chapter_two_large = 1000,
  chapter_two_boss = 2500
}
_tMoodReward = {
  none = 0,
  chapter_one_tiny = 5,
  chapter_one_small = 25,
  chapter_one_medium = 50,
  chapter_one_large = 75,
  chapter_one_boss = 100,
  chapter_two_tiny = 5,
  chapter_two_small = 25,
  chapter_two_medium = 50,
  chapter_two_large = 75,
  chapter_two_boss = 100
}
_tRewards = {
  AllChiIntro = {
    tSupport = {
      {"al", "All"},
      {"c4", "All"},
      {"gl", "All"},
      {"lightmg", "All"},
      {
        "combatairpatrol",
        "All"
      },
      {"tankbuster", "All"},
      {
        "hmmwvsofttop",
        "All"
      },
      {"ch", "Chi"},
      {"c4", "Chi"},
      {"sniperch", "Chi"},
      {"nglv50cal", "Chi"},
      {"bombingrun", "Chi"},
      {"artillery", "Chi"}
    }
  },
  AllCon001 = {
    nCash = 10000000,
    tAttitude = {
      All = _tMoodReward.chapter_two_large
    },
    tCustomRewards = {
      "[AllCon001.Terms.reward]"
    }
  },
  AllCon002 = {
    nCash = 5000000,
    tAttitude = {
      All = _tMoodReward.chapter_two_large
    }
  },
  AllCon003 = {
    nCash = 25000000,
    tAttitude = {
      All = _tMoodReward.chapter_two_boss
    },
    tCustomRewards = {
      "[AllCon003.Terms.Reward]"
    }
  },
  AllCon008 = {
    nCash = _tCashReward.chapter_two_medium,
    tAttitude = {
      All = _tMoodReward.chapter_one_small
    }
  },
  AllCon008_Milestone1 = {
    tSupport = {
      "hmmwvarmoredtow"
    }
  },
  AllCon008_Milestone2 = {
    tSupport = {
      "clusterbomb"
    }
  },
  AllCon008_Milestone3 = {
    tSupport = {"ah1z"}
  },
  AllCon050 = {
    nCash = _tCashReward.chapter_two_medium,
    tAttitude = {
      All = _tMoodReward.chapter_two_medium
    },
    tSupport = {
      "atal",
      "amal",
      "laserguidedbomb"
    },
    tEquipment = {"FuelTank10"},
    tStockpile = {
      {"carpetbomb", 2},
      {"laviiimgs", 1}
    }
  },
  AllCon052 = {
    nCash = _tCashReward.chapter_two_medium,
    tAttitude = {
      All = _tMoodReward.chapter_two_medium
    },
    tSupport = {"laviiimgs", "aa"},
    tEquipment = {"FuelTank12"},
    tStockpile = {
      {"m2a3", 1}
    }
  },
  AllCon053 = {
    nCash = _tCashReward.chapter_two_medium,
    tAttitude = {
      All = _tMoodReward.chapter_two_medium
    },
    tSupport = {"m2a3", "laviiiat"},
    tEquipment = {"FuelTank11"}
  },
  AllJob002_Milestone1 = {
    nCash = 1000000,
    tAttitude = {
      All = _tMoodReward.chapter_two_small
    },
    tSupport = {
      "hmmwvarmored50cal",
      "bombingrun"
    }
  },
  AllJob002_Milestone2 = {
    nCash = 1250000,
    tAttitude = {
      All = _tMoodReward.chapter_two_small
    }
  },
  AllJob002_Milestone3 = {
    nCash = 1500000,
    tAttitude = {
      All = _tMoodReward.chapter_two_small
    },
    tSupport = {
      "surgicalstrike"
    }
  },
  AllJob002_Milestone4 = {
    nCash = 2000000,
    tAttitude = {
      All = _tMoodReward.chapter_two_small
    }
  },
  AllJob002_Milestone5 = {
    nCash = 2500000,
    tAttitude = {
      All = _tMoodReward.chapter_two_small
    },
    tSupport = {"mh53j"}
  },
  AllJob002_Milestone6 = {
    nCash = 3000000,
    tAttitude = {
      All = _tMoodReward.chapter_two_small
    }
  },
  AllJob002_Milestone7 = {
    nCash = 4000000,
    tAttitude = {
      All = _tMoodReward.chapter_two_small
    }
  },
  AllJob002_Milestone8 = {
    nCash = 5000000,
    tAttitude = {
      All = _tMoodReward.chapter_two_small
    },
    tSupport = {
      "laviii50cal"
    }
  },
  AllJob002_Milestone9 = {
    nCash = 7000000,
    tAttitude = {
      All = _tMoodReward.chapter_two_small
    }
  },
  AllJob002_Milestone10 = {
    nCash = 10000000,
    tAttitude = {
      All = _tMoodReward.chapter_two_small
    },
    tSupport = {"moab"}
  },
  AllJob003_PerTarget = {
    nCash = _tCashReward.chapter_two_tiny,
    tAttitude = {
      All = _tMoodReward.chapter_two_tiny
    }
  },
  AllJob003_Milestone1 = {
    tSupport = {"laviii25mm"}
  },
  AllJob003_Milestone2 = {
    tSupport = {
      "daisycutter"
    },
    tStockpile = {
      {
        "cruisemissile",
        2
      }
    }
  },
  AllJob003_Milestone3 = {
    tSupport = {
      "laviiimewss"
    }
  },
  AllJob003_Milestone4 = {
    tSupport = {"carpetbomb"}
  },
  AllJob020_PerTarget = {
    nCash = _tCashReward.chapter_two_small,
    tAttitude = {
      All = _tMoodReward.chapter_two_small
    }
  },
  AllJob020_Milestone1 = {
    tSupport = {
      "hmmwvarmoredgl"
    },
    tStockpile = {
      {
        "clusterbomb",
        2
      }
    }
  },
  AllJob020_Milestone2 = {
    tSupport = {"smartbomb"},
    tStockpile = {
      {"ah1z", 1}
    }
  },
  AllJob020_Milestone3 = {
    tSupport = {
      "bunkerbuster"
    }
  },
  AllJob020_Milestone4 = {
    tSupport = {
      "cruisemissile"
    }
  },
  AllJob020_Milestone5 = {
    tSupport = {"laviiiad"}
  },
  AllJob020_Milestone6 = {
    tSupport = {
      "hmmwvavenger"
    }
  },
  AllJob020_Milestone7 = {
    tSupport = {"m1a2"}
  },
  ChiCon001 = {
    nCash = 5000000,
    tAttitude = {
      Chi = _tMoodReward.chapter_two_large
    }
  },
  ChiCon002 = {
    nCash = 10000000,
    tAttitude = {
      Chi = _tMoodReward.chapter_two_large
    }
  },
  ChiCon003 = {
    nCash = 25000000,
    tAttitude = {
      Chi = _tMoodReward.chapter_two_boss
    },
    tCustomRewards = {
      "[AllCon003.Terms.Reward]"
    }
  },
  ChiCon008 = {
    nCash = _tCashReward.chapter_two_medium,
    tAttitude = {
      Chi = _tMoodReward.chapter_two_small
    }
  },
  ChiCon008_Milestone1 = {
    tStockpile = {
      {"sx2150mlrs", 1}
    }
  },
  ChiCon008_Milestone2 = {
    tSupport = {"wz551"}
  },
  ChiCon008_Milestone3 = {
    tSupport = {"mi26ch"}
  },
  ChiCon009 = {
    nCash = _tCashReward.chapter_two_medium,
    tAttitude = {
      Chi = _tMoodReward.chapter_two_small
    }
  },
  ChiCon009_Milestone1 = {
    tSupport = {"sx2150mlrs"}
  },
  ChiCon009_Milestone2 = {
    tSupport = {
      "clusterbomb"
    }
  },
  ChiCon009_Milestone3 = {
    tSupport = {"pgz95"}
  },
  ChiCon050 = {
    nCash = _tCashReward.chapter_two_medium,
    tAttitude = {
      Chi = _tMoodReward.chapter_two_medium
    },
    tSupport = {
      "tankbuster",
      "plz45",
      "aa"
    },
    tEquipment = {"FuelTank13"},
    tStockpile = {
      {
        "rocketartillery",
        2
      }
    }
  },
  ChiCon051 = {
    nCash = _tCashReward.chapter_two_medium,
    tAttitude = {
      Chi = _tMoodReward.chapter_two_medium
    },
    tSupport = {
      "zbd2000",
      "atch",
      "fuelairbomb"
    },
    tEquipment = {"FuelTank14"},
    tStockpile = {
      {"pgz95", 1}
    }
  },
  ChiCon053 = {
    nCash = _tCashReward.chapter_two_medium,
    tAttitude = {
      Chi = _tMoodReward.chapter_two_medium
    },
    tSupport = {
      "ztz63a",
      "rocketartillery"
    },
    tEquipment = {"FuelTank9"}
  },
  ChiJob002_Milestone1 = {
    nCash = 1000000,
    tAttitude = {
      Chi = _tMoodReward.chapter_two_small
    },
    tSupport = {
      "combatairpatrol"
    }
  },
  ChiJob002_Milestone2 = {
    nCash = 1250000,
    tAttitude = {
      Chi = _tMoodReward.chapter_two_small
    }
  },
  ChiJob002_Milestone3 = {
    nCash = 1500000,
    tAttitude = {
      Chi = _tMoodReward.chapter_two_small
    },
    tStockpile = {
      {
        "clusterbomb",
        2
      }
    }
  },
  ChiJob002_Milestone4 = {
    nCash = 2000000,
    tAttitude = {
      Chi = _tMoodReward.chapter_two_small
    }
  },
  ChiJob002_Milestone5 = {
    nCash = 2500000,
    tAttitude = {
      Chi = _tMoodReward.chapter_two_small
    },
    tSupport = {
      "strategicmissile"
    }
  },
  ChiJob002_Milestone6 = {
    nCash = 3000000,
    tAttitude = {
      Chi = _tMoodReward.chapter_two_small
    }
  },
  ChiJob002_Milestone7 = {
    nCash = 4000000,
    tAttitude = {
      Chi = _tMoodReward.chapter_two_medium
    }
  },
  ChiJob002_Milestone8 = {
    nCash = 5000000,
    tAttitude = {
      Chi = _tMoodReward.chapter_two_medium
    },
    tStockpile = {
      {"wz10", 1}
    }
  },
  ChiJob002_Milestone9 = {
    nCash = 7000000,
    tAttitude = {
      Chi = _tMoodReward.chapter_two_medium
    }
  },
  ChiJob002_Milestone10 = {
    nCash = 10000000,
    tAttitude = {
      Chi = _tMoodReward.chapter_two_large
    },
    tSupport = {
      "laserguidedbomb"
    }
  },
  ChiJob003_PerTarget = {
    nCash = _tCashReward.chapter_two_tiny,
    tAttitude = {
      Chi = _tMoodReward.chapter_two_tiny
    }
  },
  ChiJob003_Milestone1 = {
    tStockpile = {
      {"ztz63a", 1}
    }
  },
  ChiJob003_Milestone2 = {
    tStockpile = {
      {
        "strategicmissile",
        2
      }
    }
  },
  ChiJob003_Milestone3 = {
    tSupport = {"ka29b"}
  },
  ChiJob003_Milestone4 = {
    tSupport = {"wz10"}
  },
  ChiJob020_PerTarget = {
    nCash = _tCashReward.chapter_two_small,
    tAttitude = {
      Chi = _tMoodReward.chapter_two_small
    }
  },
  ChiJob020_Milestone1 = {
    tSupport = {"amch"}
  },
  ChiJob020_Milestone2 = {
    tSupport = {
      "pgz95command"
    }
  },
  ChiJob020_Milestone3 = {
    tSupport = {
      "nglvgl",
      "surgicalstrike"
    }
  },
  ChiJob020_Milestone4 = {
    tSupport = {"ztz98"}
  },
  GurIntro = {
    tSupport = {
      {
        "uh1transportgr",
        "Gur"
      },
      {"gr", "Gur"},
      {
        "m151softtopgr",
        "Gur"
      }
    }
  },
  GurCon001 = {
    nCash = 850000,
    tAttitude = {
      Gur = _tMoodReward.chapter_one_large
    },
    tCustomRewards = {
      "[GurCon001.Terms.BonusPayment]"
    }
  },
  GurCon002 = {
    nCash = 750000,
    tAttitude = {
      Gur = _tMoodReward.chapter_one_large
    },
    tCustomRewards = {
      "[GurCon002.Terms.BonusPayment]"
    }
  },
  GurCon003 = {
    nCash = _tCashReward.chapter_one_medium,
    tAttitude = {
      Gur = _tMoodReward.chapter_one_small
    },
    tCustomRewards = {
      "[GurCon003.Objectives.bonus]"
    }
  },
  GurCon003_Milestone1 = {
    tStockpile = {
      {"m113aagr", 1}
    }
  },
  GurCon003_Milestone2 = {
    tSupport = {"junkers"}
  },
  GurCon003_Milestone3 = {
    tSupport = {"piranha"}
  },
  GurCon005 = {
    nCash = _tCashReward.chapter_one_medium,
    tAttitude = {
      Gur = _tMoodReward.chapter_one_medium
    },
    tSupport = {
      "endriagoattack"
    }
  },
  GurCon050 = {
    nCash = _tCashReward.chapter_one_medium,
    tAttitude = {
      Gur = _tMoodReward.chapter_one_medium
    },
    tSupport = {"m551", "sniperch"},
    tEquipment = {"FuelTank8"}
  },
  GurCon052 = {
    nCash = _tCashReward.chapter_one_medium,
    tAttitude = {
      Gur = _tMoodReward.chapter_one_medium
    },
    tSupport = {
      "endriagosuperiority"
    },
    tEquipment = {"FuelTank6"}
  },
  GurCon053 = {
    nCash = _tCashReward.chapter_one_medium,
    tAttitude = {
      Gur = _tMoodReward.chapter_one_medium
    },
    tSupport = {
      "m113gr",
      "artillery",
      "Support"
    },
    tEquipment = {"FuelTank7"}
  },
  GurJob001_PerTarget = {
    nCash = _tCashReward.chapter_one_tiny,
    tAttitude = {
      Gur = _tMoodReward.chapter_one_tiny
    }
  },
  GurJob001_Milestone1 = {
    tSupport = {"c4"}
  },
  GurJob001_Milestone2 = {
    tSupport = {
      "m15150calgr"
    }
  },
  GurJob001_Milestone3 = {
    tStockpile = {
      {
        "endriagoelite",
        1
      }
    }
  },
  GurJob001_Milestone4 = {
    tSupport = {"tankbuster"}
  },
  GurJob001_Milestone5 = {
    tSupport = {
      "endriagoelite"
    }
  },
  GurJob002_Milestone1 = {
    nCash = 150000,
    tAttitude = {
      Gur = _tMoodReward.chapter_one_small
    },
    tSupport = {
      "turbosquidgr"
    }
  },
  GurJob002_Milestone2 = {
    nCash = 200000,
    tAttitude = {
      Gur = _tMoodReward.chapter_one_small
    }
  },
  GurJob002_Milestone3 = {
    nCash = 250000,
    tAttitude = {
      Gur = _tMoodReward.chapter_one_small
    },
    tStockpile = {
      {"bombingrun", 3}
    }
  },
  GurJob002_Milestone4 = {
    nCash = 300000,
    tAttitude = {
      Gur = _tMoodReward.chapter_one_small
    }
  },
  GurJob002_Milestone5 = {
    nCash = 400000,
    tAttitude = {
      Gur = _tMoodReward.chapter_one_small
    },
    tStockpile = {
      {
        "daisycutter",
        3
      }
    }
  },
  GurJob002_Milestone6 = {
    nCash = 500000,
    tAttitude = {
      Gur = _tMoodReward.chapter_one_medium
    }
  },
  GurJob002_Milestone7 = {
    nCash = 700000,
    tAttitude = {
      Gur = _tMoodReward.chapter_one_medium
    }
  },
  GurJob002_Milestone8 = {
    nCash = 1000000,
    tAttitude = {
      Gur = _tMoodReward.chapter_one_medium
    },
    tSupport = {"bombingrun"}
  },
  GurJob002_Milestone9 = {
    nCash = 1250000,
    tAttitude = {
      Gur = _tMoodReward.chapter_one_medium
    }
  },
  GurJob002_Milestone10 = {
    nCash = 1500000,
    tAttitude = {
      Gur = _tMoodReward.chapter_one_large
    },
    tSupport = {
      "daisycutter"
    }
  },
  GurJob006_PerTarget = {
    nCash = _tCashReward.chapter_one_tiny,
    tAttitude = {
      Gur = _tMoodReward.chapter_one_tiny
    }
  },
  GurJob006_Milestone1 = {
    tStockpile = {
      {
        "m35guntruckgr",
        1
      }
    }
  },
  GurJob006_Milestone2 = {
    tStockpile = {
      {"m551", 1}
    }
  },
  GurJob006_Milestone3 = {
    tStockpile = {
      {
        "endriagosuperiority",
        1
      }
    }
  },
  GurJob006_Milestone4 = {
    tSupport = {"m35aagr"}
  },
  GurJob020_PerTarget = {
    nCash = _tCashReward.chapter_one_medium,
    tAttitude = {
      Gur = _tMoodReward.chapter_one_small
    }
  },
  GurJob020_Milestone1 = {
    tSupport = {"rpg"}
  },
  GurJob020_Milestone2 = {
    tSupport = {
      "combatairpatrol"
    }
  },
  GurJob020_Milestone3 = {
    tSupport = {
      "m35guntruckgr"
    }
  },
  GurJob020_Milestone4 = {
    tSupport = {"civilian"}
  },
  GurJob020_Milestone5 = {
    tSupport = {"m113aagr"}
  },
  OilCon001 = {
    nCash = 995000,
    tAttitude = {
      Oil = _tMoodReward.chapter_one_large
    }
  },
  OilCon002 = {
    nCash = _tCashReward.chapter_one_large,
    nFuel = _tFuelReward.chapter_one_medium,
    tAttitude = {
      Oil = _tMoodReward.chapter_one_large
    },
    tSupport = {
      {
        "turbosquidoc",
        "Oil",
        true
      },
      {
        "oc",
        "Oil",
        true
      }
    },
    tStockpile = {
      {
        "cqb",
        2,
        true
      },
      {
        "ext",
        1,
        true
      },
      {
        "c4",
        1,
        true
      },
      {
        "oc",
        2,
        true
      }
    }
  },
  OilCon003 = {
    nCash = _tCashReward.chapter_one_medium,
    tAttitude = {
      Oil = _tMoodReward.chapter_one_small
    }
  },
  OilCon003_Milestone1 = {
    tSupport = {"lightmg"}
  },
  OilCon003_Milestone2 = {
    tSupport = {"cqb"}
  },
  OilCon003_Milestone3 = {
    tStockpile = {
      {
        "coandasuperiority",
        1
      }
    }
  },
  OilCon005 = {
    nCash = _tCashReward.chapter_one_medium,
    tAttitude = {
      Oil = _tMoodReward.chapter_one_small
    }
  },
  OilCon005_Milestone1 = {
    tSupport = {"guntruckoc"}
  },
  OilCon005_Milestone2 = {
    tSupport = {"omen"}
  },
  OilCon005_Milestone3 = {
    tSupport = {
      "coandagunship"
    }
  },
  OilCon020 = {
    tCustomRewards = {
      "[OilCon020.Objectives.CustomReward]"
    }
  },
  OilCon021 = {
    nCash = 25000,
    tAttitude = {
      Oil = _tMoodReward.chapter_one_small
    }
  },
  OilCon050 = {
    nCash = _tCashReward.chapter_one_medium,
    tAttitude = {
      Oil = _tMoodReward.chapter_one_medium
    },
    tSupport = {
      "uptankbuster",
      "ext",
      "c4"
    },
    tEquipment = {"FuelTank1"}
  },
  OilCon051 = {
    nCash = _tCashReward.chapter_one_medium,
    tAttitude = {
      Oil = _tMoodReward.chapter_one_medium
    },
    tEquipment = {"FuelTank3"},
    tSupport = {
      "stingrayii",
      "upclusterbomb"
    }
  },
  OilCon052 = {
    nCash = _tCashReward.chapter_one_medium,
    tAttitude = {
      Oil = _tMoodReward.chapter_one_medium
    },
    tSupport = {
      "coandaattack",
      "upcombatairpatrol"
    },
    tEquipment = {"FuelTank2"}
  },
  OilJob004_PerTarget = {
    nCash = _tCashReward.chapter_one_tiny,
    tAttitude = {
      Oil = _tMoodReward.chapter_one_tiny
    }
  },
  OilJob004_Milestone1 = {
    tStockpile = {
      {"extgl", 1}
    }
  },
  OilJob004_Milestone2 = {
    tStockpile = {
      {"guntruckoc", 1}
    }
  },
  OilJob004_Milestone3 = {
    tStockpile = {
      {
        "coandagunship",
        1
      }
    }
  },
  OilJob004_Milestone4 = {
    tSupport = {"extgl"}
  },
  OilJob008_PerTarget = {
    nCash = _tCashReward.chapter_one_small,
    tAttitude = {
      Oil = _tMoodReward.chapter_one_small
    }
  },
  OilJob008_Milestone1 = {
    tStockpile = {
      {"gl", 2}
    }
  },
  OilJob008_Milestone2 = {
    tStockpile = {
      {"omen", 1}
    }
  },
  OilJob008_Milestone3 = {
    tSupport = {
      "coandatransport"
    }
  },
  OilJob008_Milestone4 = {
    tSupport = {"gl"}
  },
  OilJob008_Milestone5 = {
    tSupport = {"luxury"}
  },
  OilJob011_Milestone1 = {
    nCash = 150000,
    tAttitude = {
      Oil = _tMoodReward.chapter_one_small
    },
    tStockpile = {
      {"stingrayii", 1}
    }
  },
  OilJob011_Milestone2 = {
    nCash = 200000,
    tAttitude = {
      Oil = _tMoodReward.chapter_one_small
    }
  },
  OilJob011_Milestone3 = {
    nCash = 250000,
    tAttitude = {
      Oil = _tMoodReward.chapter_one_small
    },
    tStockpile = {
      {
        "coandaattack",
        1
      }
    }
  },
  OilJob011_Milestone4 = {
    nCash = 300000,
    tAttitude = {
      Oil = _tMoodReward.chapter_one_small
    }
  },
  OilJob011_Milestone5 = {
    nCash = 400000,
    tAttitude = {
      Oil = _tMoodReward.chapter_one_small
    },
    tSupport = {"tankbuster"}
  },
  OilJob011_Milestone6 = {
    nCash = 500000,
    tAttitude = {
      Oil = _tMoodReward.chapter_one_medium
    }
  },
  OilJob011_Milestone7 = {
    nCash = 700000,
    tAttitude = {
      Oil = _tMoodReward.chapter_one_medium
    }
  },
  OilJob011_Milestone8 = {
    nCash = 1000000,
    tAttitude = {
      Oil = _tMoodReward.chapter_one_medium
    },
    tSupport = {
      "combatairpatrol"
    }
  },
  OilJob011_Milestone9 = {
    nCash = 1250000,
    tAttitude = {
      Oil = _tMoodReward.chapter_one_medium
    }
  },
  OilJob011_Milestone10 = {
    nCash = 1500000,
    tAttitude = {
      Oil = _tMoodReward.chapter_one_large
    },
    tSupport = {
      "coandasuperiority"
    }
  },
  PmcCon001 = {
    nCash = _tCashReward.none
  },
  PmcCon002 = {nCash = 650000},
  PmcCon003 = {
    nCash = _tCashReward.none
  },
  PmcCon004 = {
    nCash = 25000000,
    tSupport = {"nuke"}
  },
  PmcCon013 = {
    nWagerPercent = 5,
    nWagerMinPercent = 5,
    nWagerMaxPercent = 20,
    nWagerMax = 5000000
  },
  PmcCon015 = {
    nWager = 100000,
    nWagerMin = 100000,
    nWagerMax = 500000
  },
  PmcCon015_Milestone1 = {
    tStockpile = {
      {
        "sidecarmotorcycle",
        1
      }
    }
  },
  PmcCon015_Milestone2 = {
    tStockpile = {
      {
        "valiantpython",
        1
      }
    }
  },
  PmcCon015_Milestone3 = {
    tStockpile = {
      {"tankbike", 1}
    }
  },
  PmcCon016 = {
    nWager = 100000,
    nWagerMin = 100000,
    nWagerMax = 500000
  },
  PmcCon016_Milestone1 = {
    tStockpile = {
      {
        "mattiaschopper",
        1
      }
    }
  },
  PmcCon016_Milestone2 = {
    tStockpile = {
      {
        "dsvscoutvehicle",
        1
      }
    }
  },
  PmcCon016_Milestone3 = {
    tStockpile = {
      {
        "panhardassault",
        1
      }
    }
  },
  PmcCon018 = {
    nWager = 100000,
    nWagerMin = 100000,
    nWagerMax = 1000000
  },
  PmcCon018_Milestone1 = {
    tStockpile = {
      {
        "buggyhellfire",
        1
      }
    }
  },
  PmcCon018_Milestone2 = {
    tStockpile = {
      {
        "patrolboatpmc",
        1
      }
    }
  },
  PmcCon018_Milestone3 = {
    tStockpile = {
      {
        "veyronassault",
        1
      }
    }
  },
  PmcCon031 = {
    nWager = 1000,
    nWagerMin = 1000,
    nWagerMax = 100000
  },
  PmcCon031_Milestone1 = {
    tStockpile = {
      {"fiona", 1}
    }
  },
  PmcCon032 = {
    nWager = 1000,
    nWagerMin = 1000,
    nWagerMax = 100000
  },
  PmcCon032_Milestone1 = {
    tStockpile = {
      {"fiona", 2}
    }
  },
  PmcCon033 = {
    nWager = 10000,
    nWagerMin = 10000,
    nWagerMax = 100000
  },
  PmcCon033_Milestone1 = {
    tStockpile = {
      {"fiona", 3}
    }
  },
  PmcCon034 = {
    nWager = 10000,
    nWagerMin = 10000,
    nWagerMax = 100000
  },
  PmcCon034_Milestone1 = {
    tStockpile = {
      {"fiona", 4}
    }
  },
  VzaCon001 = {
    nCash = _tCashReward.none
  },
  PmcJob001_Milestone1 = {
    tSupport = {"tankbike"}
  },
  PmcJob001_Milestone2 = {
    tSupport = {
      "mattiaschopper"
    }
  },
  PmcJob001_Milestone3 = {
    tSupport = {
      "panhardassault"
    }
  },
  PmcJob001_Milestone4 = {
    tSupport = {
      "veyronassault"
    }
  },
  PmcJob001_Milestone5 = {
    tSupport = {
      "valiantpython"
    }
  },
  PmcJob001_Milestone6 = {
    tSupport = {
      "sidecarmotorcycle"
    }
  },
  PmcJob001_Milestone7 = {
    tSupport = {
      "dsvscoutvehicle"
    }
  },
  PmcJob001_Milestone8 = {
    tSupport = {
      "patrolboatpmc"
    }
  },
  PmcJob001_Milestone9 = {
    tSupport = {
      "buggyhellfire"
    }
  },
  MecCon001 = {
    tSupport = {
      {
        "monstertruck",
        "Pmc",
        true
      }
    }
  },
  PirIntro = {
    tSupport = {
      {"sniperru", "Pir"},
      {
        "m15150calvz",
        "Pir"
      },
      {"pr", "Pir"},
      {
        "alouette3transportvz",
        "Pir"
      }
    }
  },
  PirCon001 = {
    nCash = _tCashReward.chapter_one_small,
    tAttitude = {
      Pir = _tMoodReward.chapter_one_small
    }
  },
  PirCon001_Milestone1 = {
    tSupport = {"jetskiciv"}
  },
  PirCon001_Milestone2 = {
    tSupport = {"dinghy", "monster"}
  },
  PirCon001_Milestone3 = {
    tSupport = {"sports", "buggypr"}
  },
  PirCon002 = {
    nCash = 20000,
    tAttitude = {
      Pir = _tMoodReward.chapter_one_small
    }
  },
  PirCon002_Milestone1 = {
    tSupport = {"t300m60"}
  },
  PirCon002_Milestone2 = {
    tSupport = {
      "alouette3attackpr"
    }
  },
  PirCon002_Milestone3 = {
    tSupport = {"m113vz"}
  },
  PirCon003 = {
    nCash = 30000,
    tAttitude = {
      Pir = _tMoodReward.chapter_one_small
    }
  },
  PirCon003_Milestone1 = {
    tSupport = {
      "m113jammervz"
    }
  },
  PirCon003_Milestone2 = {
    tSupport = {
      "alouette3attackvz"
    }
  },
  PirCon003_Milestone3 = {
    tSupport = {"m35aavz"}
  },
  PirCon004 = {
    nCash = _tCashReward.chapter_one_medium,
    tAttitude = {
      Pir = _tMoodReward.chapter_one_small
    }
  },
  PirCon004_Milestone1 = {
    tSupport = {
      "m35guntruckvz"
    }
  },
  PirCon004_Milestone2 = {
    tSupport = {
      "alouette3elite"
    }
  },
  PirCon004_Milestone3 = {
    tSupport = {"amx30aa"}
  },
  PirCon051 = {
    nCash = _tCashReward.chapter_one_medium,
    tAttitude = {
      Pir = _tMoodReward.chapter_one_medium
    },
    tSupport = {
      "scorpion90",
      "patrolboatvz"
    },
    tEquipment = {"FuelTank5"}
  },
  PirCon052 = {
    nCash = _tCashReward.chapter_one_medium,
    tAttitude = {
      Pir = _tMoodReward.chapter_one_medium
    },
    tSupport = {"mi35", "speedboat"},
    tEquipment = {"FuelTank4"}
  },
  PirJob012_Milestone1 = {
    nCash = 100000,
    tAttitude = {
      Pir = _tMoodReward.chapter_one_small
    },
    tSupport = {
      "alouette3transportpr"
    }
  },
  PirJob012_Milestone2 = {
    nCash = 125000,
    tAttitude = {
      Pir = _tMoodReward.chapter_one_small
    }
  },
  PirJob012_Milestone3 = {
    nCash = 150000,
    tAttitude = {
      Pir = _tMoodReward.chapter_one_small
    },
    tSupport = {
      "m15150calvz"
    }
  },
  PirJob012_Milestone4 = {
    nCash = 175000,
    tAttitude = {
      Pir = _tMoodReward.chapter_one_small
    }
  },
  PirJob012_Milestone5 = {
    nCash = 200000,
    tAttitude = {
      Pir = _tMoodReward.chapter_one_small
    },
    tSupport = {"bike"}
  },
  PirJob012_Milestone6 = {
    nCash = 250000,
    tAttitude = {
      Pir = _tMoodReward.chapter_one_medium
    }
  },
  PirJob012_Milestone7 = {
    nCash = 300000,
    tAttitude = {
      Pir = _tMoodReward.chapter_one_medium
    }
  },
  PirJob012_Milestone8 = {
    nCash = 500000,
    tAttitude = {
      Pir = _tMoodReward.chapter_one_medium
    },
    tSupport = {
      "alouette3superiority"
    }
  },
  PirJob012_Milestone9 = {
    nCash = 750000,
    tAttitude = {
      Pir = _tMoodReward.chapter_one_medium
    }
  },
  PirJob012_Milestone10 = {
    nCash = 1000000,
    tAttitude = {
      Pir = _tMoodReward.chapter_one_large
    },
    tSupport = {"amx30elite"}
  },
  PirJob020_PerTarget = {
    nCash = _tCashReward.chapter_one_small,
    tAttitude = {
      Pir = _tMoodReward.chapter_one_small
    }
  },
  PirJob020_Milestone1 = {
    tSupport = {"covert"}
  },
  PirJob020_Milestone2 = {
    tSupport = {"m113aavz"}
  },
  PirJob020_Milestone3 = {
    tSupport = {"utility"}
  },
  PirJob020_Milestone4 = {
    tSupport = {"amx30"}
  },
  PirJob020_Milestone5 = {
    tSupport = {"mi26vz"}
  }
}

function Init()
  for sRewardKey, tRewards in pairs(_tRewards) do
    local sFactionId
    local sMissionId = string.sub(sRewardKey, 1, 9)
    if sMissionId then
      local tMissionConfig = WifMissionData.tMissionData[sMissionId]
      if tMissionConfig then
        sFactionId = tMissionConfig.sFactionId
      end
      tRewards.sMissionId = sMissionId
    end
    if sFactionId then
      tRewards.sFactionId = sFactionId
    end
    if tRewards.tSupport then
      for i, vItem in ipairs(tRewards.tSupport) do
        if type(vItem) == "string" then
          vItem = {vItem}
        end
        ASSERT(type(vItem) == "table")
        if sFactionId and not vItem[2] then
          vItem[2] = sFactionId
        end
        if not vItem[3] then
          vItem[3] = false
        end
        tRewards.tSupport[i] = vItem
      end
    end
    if tRewards.tEquipment then
      for i, vItem in ipairs(tRewards.tEquipment) do
        if type(vItem) == "string" then
          vItem = {vItem}
        end
        ASSERT(type(vItem) == "table")
        if sFactionId and not vItem[2] then
          vItem[2] = sFactionId
        end
        if not vItem[3] then
          vItem[3] = false
        end
        tRewards.tEquipment[i] = vItem
      end
    end
    if tRewards.tStockpile then
      for i, vItem in ipairs(tRewards.tStockpile) do
        if type(vItem) == "string" then
          vItem = {vItem}
        end
        ASSERT(type(vItem) == "table")
        if not vItem[2] then
          vItem[2] = 1
        end
        if not vItem[3] then
          vItem[3] = false
        end
        tRewards.tStockpile[i] = vItem
      end
    end
  end
end

function GetRewards(sRewardKey)
  return _tRewards[sRewardKey]
end

function GetAllPotentialShopItems(sFactionId)
  if not gtAllSupport then
    gtAllSupport = {}
    gtAllEquipment = {}
  end
  if not gtAllSupport[sFactionId] then
    local tSupport = {}
    local tEquipment = {}
    for sRewardKey, tRewards in pairs(_tRewards) do
      if type(tRewards.tSupport) == "table" then
        for i, tItem in ipairs(tRewards.tSupport) do
          local sId = tItem[1]
          local sThisFaction = tItem[2]
          if sFactionId == sThisFaction and MrxSupportData.tSupportData[sId] then
            table.insert(tSupport, sId)
          end
        end
      end
      if sFactionId == tRewards.sFactionId then
        local sType = type(tRewards.tEquipment)
        if sType == "table" then
          for _, vId in ipairs(tRewards.tEquipment) do
            if type(vId) == "table" then
              for i, tItem in ipairs(tRewards.tEquipment) do
                local sId = tItem[1]
                local sThisFaction = tItem[2]
                table.insert(tEquipment, sId)
              end
            elseif WifEquipmentData.GetEquipmentData(vId) then
              table.insert(tEquipment, vId)
            end
          end
        elseif sType == "string" and WifEquipmentData.GetEquipmentData(tRewards.tEquipment) then
          table.insert(tEquipment, tRewards.tEquipment)
        end
      end
    end
    gtAllSupport[sFactionId] = tSupport
    gtAllEquipment[sFactionId] = tEquipment
  end
  return gtAllSupport[sFactionId], gtAllEquipment[sFactionId]
end

function DispenseRewards(tRewards, bDisableFanfares)
  local nCash
  if type(tRewards.nCashOverride) == "number" then
    nCash = tRewards.nCashOverride
    tRewards.nCashOverride = nil
  elseif type(tRewards.nCash) == "number" then
    nCash = tRewards.nCash
    if _bHalveCashReward then
      nCash = nCash * 0.5
    end
  end
  if nCash then
    local sReason
    local sMissionId = tRewards.sMissionId
    if sMissionId and WifMissionData.IsMissionAContract(sMissionId) then
      sReason = "[Generic.Contracts]"
    end
    MrxPmc.AddCashQty(nCash, nil, sReason)
  end
  if type(tRewards.nFuel) == "number" then
    MrxPmc.AddFuelQty(tRewards.nFuel)
  end
  if not Net.IsClient() then
    if type(tRewards.tAttitude) == "table" then
      for sFactionName, nAttitudeValue in pairs(tRewards.tAttitude) do
        MrxFactionManager.ChangeRelation(sFactionName, "Pmc", nAttitudeValue)
      end
    end
    if type(tRewards.tSupport) == "table" or type(tRewards.tEquipment) == "table" then
      local tSupportByFaction = {}
      local tFanfareItems = {}
      if type(tRewards.tSupport) == "table" then
        for i, tItem in ipairs(tRewards.tSupport) do
          local sId = tItem[1]
          local sFaction = tItem[2]
          if tSupportByFaction[sFaction] then
            table.insert(tSupportByFaction[sFaction], sId)
          else
            tSupportByFaction[sFaction] = {sId}
          end
          table.insert(tFanfareItems, {sFactionId = sFaction, sSupportId = sId})
        end
        for sFaction, tItems in pairs(tSupportByFaction) do
          MrxSupportData.Add(tItems, sFaction)
        end
      end
      if type(tRewards.tEquipment) == "table" then
        for i, tItem in ipairs(tRewards.tEquipment) do
          local sId = tItem[1]
          local sFaction = tItem[2]
          table.insert(tFanfareItems, {sFactionId = sFaction, sEquipmentId = sId})
          WifEquipmentData.UnlockItem(sId, sFaction)
        end
      end
      if not bDisableFanfares then
        MrxUnlockFanfare.AddUnlockedItems("support", tFanfareItems)
      end
    end
  end
  if type(tRewards.tStockpile) == "table" then
    local tFanfareItems = {}
    for i, tItem in ipairs(tRewards.tStockpile) do
      local sId = tItem[1]
      local nQty = tItem[2]
      MrxPmc.AddSupportQty(sId, nQty)
      table.insert(tFanfareItems, {sSupportId = sId, nQty = nQty})
    end
    if not bDisableFanfares then
      MrxUnlockFanfare.AddUnlockedItems("stockpile", tFanfareItems)
    end
  end
end

function DispenseAllRewards()
  for sRewardKey, tRewardData in pairs(_tRewards) do
    DispenseRewards(tRewardData, true)
  end
end

EVENT_GRANTREWARDKEY = 0

function GetRewardKeyFromHash(uNameHash)
  for sName, tData in pairs(_tRewards) do
    if String.GetHash(sName) == uNameHash then
      return sName
    end
  end
  return nil
end

function NetEventCallback(nEventId, tArgs)
  if nEventId == EVENT_GRANTREWARDKEY then
    local sRewardKey = GetRewardKeyFromHash(tArgs[1])
    if sRewardKey then
      GrantRewardKey(sRewardKey, tArgs[2])
    end
  end
end

function GrantRewardKey(sRewardKey, nCashOverride)
  local tRewards = GetRewards(sRewardKey)
  if tRewards then
    if Net.IsServer() then
      Net.SendCustomEvent("MrxRewardData", EVENT_GRANTREWARDKEY, {
        sRewardKey,
        tRewards.nCashOverride2
      })
      DispenseRewards(tRewards)
    end
    if Net.IsClient() then
      if nCashOverride then
        tRewards.nCashOverride = nCashOverride
      end
      DispenseRewards(tRewards, true)
    end
  end
end

function EnableCashRewardHalving(bEnable)
  _bHalveCashReward = bEnable
end

function SaveSingleton()
  local tSaveData = {}
  for sRewardKey, tRewards in pairs(_tRewards) do
    if tRewards.nWager then
      tSaveData[sRewardKey] = {
        nWager = tRewards.nWager
      }
    end
  end
  return tSaveData
end

function LoadSingleton(tSaveData)
  if tSaveData then
    for sRewardKey, tData in pairs(tSaveData) do
      local tRewards = _tRewards[sRewardKey]
      if tRewards then
        for key, value in pairs(tData) do
          tRewards[key] = value
        end
      end
    end
  end
end

function GetWagerData(tRewards)
  local function _Round(nValue)
    return math.floor(nValue / 1000) * 1000
  end
  
  local tWager
  if tRewards.nWagerPercent then
    tWager = {}
    local nCash = MrxPmc.GetCashQty()
    tWager.nWager = _Round(nCash * (tRewards.nWagerPercent / 100))
    if tRewards.nWager ~= nil and tWager.nWager > tRewards.nWager then
      tWager.nWager = tRewards.nWager
    end
    tWager.nWagerMin = _Round(nCash * (tRewards.nWagerMinPercent / 100))
    if tRewards.nWagerMin ~= nil and tWager.nWagerMin < tRewards.nWagerMin then
      tWager.nWagerMin = tRewards.nWagerMin
    end
    tWager.nWagerMax = _Round(nCash * (tRewards.nWagerMaxPercent / 100))
    if tRewards.nWagerMax ~= nil and tWager.nWagerMax > tRewards.nWagerMax then
      tWager.nWagerMax = tRewards.nWagerMax
    end
    if tWager.nWagerMin > tWager.nWagerMax then
      tWager.nWagerMin = _Round(tWager.nWagerMax * 0.25)
    end
    tWager.nCash = nCash
  end
  if tRewards.nWager then
    tWager = {}
    tWager.nWager = tRewards.nWager
    tWager.nWagerMin = tRewards.nWagerMin
    tWager.nWagerMax = tRewards.nWagerMax
    tWager.nCash = MrxPmc.GetCashQty()
  end
  if tWager then
    if tWager.nWager < tWager.nWagerMin then
      tWager.nWager = tWager.nWagerMin
    end
    local nDefaultWager = tWager.nWager
    if tWager.nCash < tWager.nWagerMin then
      nDefaultWager = tWager.nWagerMin
    elseif tWager.nCash < tWager.nWager then
      nDefaultWager = tWager.nCash
    end
    tWager.nDefaultWager = _Round(nDefaultWager)
  end
  return tWager
end

function GenerateRewardString(sMissionId)
  if not sMissionId then
    return nil
  end
  local sRewards
  local tRewards = GetRewards(sMissionId)
  if tRewards then
    sRewards = _GenerateStringFromRewardData(tRewards)
  end
  local bLevel = WifMissionData.IsMissionAContract(sMissionId) and WifMissionData.GetMissionRepeatable(sMissionId)
  local tMilestones, sSingleMilestone, sPluralMilestone = WifMissionData.GetMissionMilestoneData(sMissionId)
  if tMilestones then
    for n, tMilestoneData in pairs(tMilestones) do
      local sMilestoneString
      if bLevel then
        sMilestoneString = _FormatLevelMilestoneString(tMilestoneData)
      else
        sMilestoneString = _FormatMilestoneString(tMilestoneData, sSingleMilestone, sPluralMilestone)
      end
      if sMilestoneString then
        sRewards = (sRewards or "") .. sMilestoneString
      end
    end
  end
  if tRewards and tRewards.tCustomRewards then
    local sCustomRewards = ""
    for n, sCustRew in pairs(tRewards.tCustomRewards) do
      sCustomRewards = sCustomRewards .. sCustRew .. "\n"
    end
    sRewards = (sRewards or "") .. sCustomRewards
  end
  if "" == sRewards then
    return nil
  end
  return sRewards
end

function AddCustomReward(sMissionId, sCustomReward)
  local tRewards = GetRewards(sMissionId)
  if not tRewards.tCustomRewards then
    tRewards.tCustomRewards = {}
  end
  table.insert(tRewards.tCustomRewards, sCustomReward)
end

function _GenerateStringFromRewardData(tRewards, sPrependedString, sAppendedString)
  local sRewards = ""
  sAppendedString = sAppendedString or ""
  sAppendedString = " " .. sAppendedString .. "\n"
  sPrependedString = sPrependedString or ""
  if tRewards.nCash and tRewards.nCash > 0 then
    local sCashText = "[cash] " .. MrxUtil.FormatMoney(tRewards.nCash)
    sRewards = sRewards .. sPrependedString .. sCashText .. sAppendedString
  end
  if tRewards.nFuel and 0 < tRewards.nFuel then
    local sFuelText = "[fuel] " .. tRewards.nFuel
    sRewards = sRewards .. sPrependedString .. sFuelText .. sAppendedString
  end
  if tRewards.tSupport then
    local sSupportString
    for n, tSupportItem in pairs(tRewards.tSupport) do
      local sSupportId = tSupportItem[1]
      local sFactionId = tSupportItem[2]
      local bHidden = tSupportItem[3]
      if not bHidden then
        if sSupportId then
          sSupportString = _GetPrintableSupportString(sSupportId)
          if sFactionId then
            sSupportString = sSupportString .. "\n" .. sPrependedString .. "[indent] " .. "([Briefing.Shop]: " .. MrxFactionManager.GetInlineIcon(sFactionId) .. ")"
          end
        end
        if sSupportString then
          sRewards = sRewards .. sPrependedString .. sSupportString .. sAppendedString
        end
      end
    end
  end
  if tRewards.tEquipment then
    local sEquipString
    for n, tEquipItem in pairs(tRewards.tEquipment) do
      local sEquipId = tEquipItem[1]
      local sFactionId = tEquipItem[2]
      local bHidden = tEquipItem[3]
      if not bHidden then
        sEquipString = _GetPrintableEquipmentString(sEquipId)
        if sFactionId then
          sEquipString = sEquipString .. "\n" .. sPrependedString .. "[indent] " .. "([Briefing.Shop]: " .. MrxFactionManager.GetInlineIcon(sFactionId) .. ")"
        end
        if sEquipString then
          sRewards = sRewards .. sPrependedString .. sEquipString .. sAppendedString
        end
      end
    end
  end
  if tRewards.tStockpile then
    local sStockpileString
    for n, tStockpileItem in pairs(tRewards.tStockpile) do
      local sSupportId = tStockpileItem[1]
      local nQty = tStockpileItem[2]
      local bHidden = tStockpileItem[3]
      if not bHidden then
        if sSupportId then
          sStockpileString = _GetPrintableSupportString(sSupportId)
          if nQty and sStockpileString then
            sStockpileString = sStockpileString .. " (x " .. nQty .. ")"
          end
        end
        if sStockpileString then
          sRewards = sRewards .. sPrependedString .. sStockpileString .. sAppendedString
        end
      end
    end
  end
  return sRewards
end

_bPrintRewardType = true

function _GetPrintableSupportString(sSupportId)
  local sReturn
  local tSupportData = MrxSupportData.tSupportData[sSupportId]
  if tSupportData and tSupportData.sName then
    sReturn = tSupportData.sName
  else
    return nil
  end
  if _bPrintRewardType then
    local tTypeToMarkupCode = {
      Airstrike = "[airstrike]",
      Supply = "[supply]",
      Light = "[vehmlight]",
      Heavy = "[vehmheavy]",
      Civilian = "[vehcivilian]",
      Boat = "[vehboat]",
      Heli = "[vehheli]"
    }
    if tSupportData and tSupportData.sType then
      local sMarkupCode = tTypeToMarkupCode[tSupportData.sType]
      if sMarkupCode then
        sReturn = sMarkupCode .. " " .. sReturn
      end
    end
  end
  return sReturn
end

function _GetPrintableEquipmentString(sEquipId)
  local tEquipmentData = WifEquipmentData.GetEquipmentData(sEquipId)
  if tEquipmentData and _bPrintRewardType then
    if tEquipmentData.nType == WifEquipmentData.knTypeFuelTank then
      return "[fuelsilo] " .. "[Generic.FuelSilo]"
    elseif tEquipmentData.nType == WifEquipmentData.knTypeGrapplingHook then
      return WifEquipmentData.GetPlayerVisibleName(sEquipId)
    end
  end
  return sEquipId
end

function _FormatMilestoneString(tMilestoneData, sSingle, sPlural)
  local nMilestone = tMilestoneData.nMilestone
  local sMilestoneKey = tMilestoneData.sKey
  local tRewardData = GetRewards(sMilestoneKey)
  local sRewards
  if not tRewardData then
    return nil
  end
  local sPrepend
  if 1 == nMilestone then
    if sSingle then
      sPrepend = _FormatMilestoneSuffix(sSingle, nMilestone)
    end
  elseif sPlural then
    sPrepend = _FormatMilestoneSuffix(sPlural, nMilestone)
  end
  if not sPrepend then
    if 1 == nMilestone then
      sPrepend = nMilestone .. " [PDA.Map.Target]"
    else
      sPrepend = nMilestone .. " [PDA.Map.Targets]"
    end
  end
  sPrepend = sPrepend .. ":\n"
  if WifMissionFlow.HasKey(sMilestoneKey) then
    sPrepend = "[check1] " .. sPrepend
  else
    sPrepend = "[check0] " .. sPrepend
  end
  return sPrepend .. _GenerateStringFromRewardData(tRewardData, "[indent] ")
end

function _FormatMilestoneSuffix(sSuffix, nNumber)
  if "string" ~= type(sSuffix) or "number" ~= type(nNumber) then
    return
  end
  local sReturn = sSuffix
  return string.format("[%s:%d]", sReturn, nNumber)
end

function _FormatLevelMilestoneString(tMilestoneData)
  local sMilestoneKey = tMilestoneData.sKey
  local tRewardData = GetRewards(sMilestoneKey)
  if not tRewardData then
    return
  end
  local sPrepend = "[Generic.Level] " .. tMilestoneData.nMilestone .. ":\n"
  if WifMissionFlow.HasKey(sMilestoneKey) then
    sPrepend = "[check1] " .. sPrepend
  else
    sPrepend = "[check0] " .. sPrepend
  end
  return sPrepend .. _GenerateStringFromRewardData(tRewardData, "[indent] ")
end
