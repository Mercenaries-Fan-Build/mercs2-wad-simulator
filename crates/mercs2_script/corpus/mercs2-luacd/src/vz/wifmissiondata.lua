knPdaSortOrderActiveContract = 1
knPdaSortOrderCritPathContract = 2
knPdaSortOrderContract = 3
knPdaSortOrderVerifySet = 4
knPdaSortOrderDestroySet = 5
knPdaSortOrderStandingBty = 6
knPdaSortOrderBillboards = 7
tMissionData = {
  AllCon001 = {
    sModuleName = "AllCon001",
    sFactionId = "All",
    tMaterielScale = {Chi = 1},
    sStarter = "AllStarter0",
    bCriticalPathMission = true
  },
  AllCon002 = {
    sModuleName = "AllCon002",
    sFactionId = "All",
    tMaterielScale = {Chi = 1},
    sStarter = "AllStarter0",
    bCriticalPathMission = true
  },
  AllCon003 = {
    sModuleName = "AllCon003",
    sFactionId = "All",
    tMaterielScale = {Chi = 1},
    sStarter = "AllStarter0",
    tLayers = {
      "Vz_State_AllCon003",
      "vz_state_AllCon003_and_ChiCon003_Pristine",
      "vz_state_AllCon003_Pristine",
      "vz_state_ChiCon003_Pristine"
    },
    bCriticalPathMission = true
  },
  AllCon008 = {
    sModuleName = "AllCon008",
    sFactionId = "All",
    tMaterielScale = {Chi = 1},
    sStarter = "AllStarter2",
    bRepeatable = true,
    nLevels = 3,
    tMilestones = {
      {
        nMilestone = 1,
        sKey = "AllCon008_Milestone1"
      },
      {
        nMilestone = 2,
        sKey = "AllCon008_Milestone2"
      },
      {
        nMilestone = 3,
        sKey = "AllCon008_Milestone3"
      }
    }
  },
  AllCon050 = {
    sModuleName = "AllCon050",
    sFactionId = "All",
    sStarter = "AllStarter1",
    sTitle = "[AllCon050.Title]",
    bCriticalPathMission = true
  },
  AllCon052 = {
    sModuleName = "AllCon052",
    sFactionId = "All",
    sStarter = "AllStarter2",
    sTitle = "[AllCon052.Title]"
  },
  AllCon053 = {
    sModuleName = "AllCon053",
    sFactionId = "All",
    sStarter = "AllStarter3",
    sTitle = "[AllCon053.Title]"
  },
  AllJob002 = {
    sModuleName = "AllJob002",
    sFactionId = "All",
    tMilestones = {
      {
        nMilestone = 1,
        sKey = "AllJob002_Milestone1"
      },
      {
        nMilestone = 2,
        sKey = "AllJob002_Milestone2"
      },
      {
        nMilestone = 3,
        sKey = "AllJob002_Milestone3"
      },
      {
        nMilestone = 4,
        sKey = "AllJob002_Milestone4"
      },
      {
        nMilestone = 5,
        sKey = "AllJob002_Milestone5"
      },
      {
        nMilestone = 6,
        sKey = "AllJob002_Milestone6"
      },
      {
        nMilestone = 7,
        sKey = "AllJob002_Milestone7"
      },
      {
        nMilestone = 8,
        sKey = "AllJob002_Milestone8"
      },
      {
        nMilestone = 9,
        sKey = "AllJob002_Milestone9"
      },
      {
        nMilestone = 10,
        sKey = "AllJob002_Milestone10"
      }
    },
    sPdaTexture = "icon_verify_2_mc",
    nPdaSortOrder = knPdaSortOrderVerifySet
  },
  AllJob003 = {
    sModuleName = "AllJob003",
    sFactionId = "All",
    bCompletable = false,
    tMilestones = {
      {
        nMilestone = 3,
        sKey = "AllJob003_Milestone1"
      },
      {
        nMilestone = 10,
        sKey = "AllJob003_Milestone2"
      },
      {
        nMilestone = 25,
        sKey = "AllJob003_Milestone3"
      },
      {
        nMilestone = 50,
        sKey = "AllJob003_Milestone4"
      }
    },
    sPdaTexture = "icon_destroy_2_mc",
    nPdaSortOrder = knPdaSortOrderStandingBty
  },
  AllJob020 = {
    sModuleName = "AllJob020",
    sFactionId = "All",
    tMilestones = {
      {
        nMilestone = 1,
        sKey = "AllJob020_Milestone1"
      },
      {
        nMilestone = 3,
        sKey = "AllJob020_Milestone2"
      },
      {
        nMilestone = 6,
        sKey = "AllJob020_Milestone3"
      },
      {
        nMilestone = 10,
        sKey = "AllJob020_Milestone4"
      },
      {
        nMilestone = 15,
        sKey = "AllJob020_Milestone5"
      },
      {
        nMilestone = 20,
        sKey = "AllJob020_Milestone6"
      },
      {
        nMilestone = 24,
        sKey = "AllJob020_Milestone7"
      }
    },
    sPdaTexture = "icon_destroy_2_mc",
    nPdaSortOrder = knPdaSortOrderDestroySet
  },
  ChiCon001 = {
    sModuleName = "ChiCon001",
    sFactionId = "Chi",
    tMaterielScale = {All = 1},
    sStarter = "ChiStarter0",
    bCriticalPathMission = true
  },
  ChiCon002 = {
    sModuleName = "ChiCon002",
    sFactionId = "Chi",
    tMaterielScale = {All = 1, Oil = 1},
    sStarter = "ChiStarter0",
    tLayers = {
      "Vz_State_ChiCon002",
      "Vz_State_ChiCon002_Pristine"
    },
    bCriticalPathMission = true
  },
  ChiCon003 = {
    sModuleName = "ChiCon003",
    sFactionId = "Chi",
    tMaterielScale = {All = 1},
    sStarter = "ChiStarter0",
    bCriticalPathMission = true,
    tLayers = {
      "Vz_State_ChiCon003",
      "vz_state_AllCon003_and_ChiCon003_Pristine",
      "vz_state_ChiCon003_Pristine",
      "vz_state_alljob001_02_captured"
    }
  },
  ChiCon008 = {
    sModuleName = "ChiCon008",
    sFactionId = "Chi",
    tMaterielScale = {All = 1},
    sStarter = "ChiStarter2",
    bRepeatable = true,
    nLevels = 3,
    tMilestones = {
      {
        nMilestone = 1,
        sKey = "ChiCon008_Milestone1"
      },
      {
        nMilestone = 2,
        sKey = "ChiCon008_Milestone2"
      },
      {
        nMilestone = 3,
        sKey = "ChiCon008_Milestone3"
      }
    },
    tLayers = {
      "vz_state_ChiCon008_a"
    }
  },
  ChiCon009 = {
    sModuleName = "ChiCon009",
    sFactionId = "Chi",
    sStarter = "ChiStarter4",
    bRepeatable = true,
    nLevels = 3,
    tMilestones = {
      {
        nMilestone = 1,
        sKey = "ChiCon009_Milestone1"
      },
      {
        nMilestone = 2,
        sKey = "ChiCon009_Milestone2"
      },
      {
        nMilestone = 3,
        sKey = "ChiCon009_Milestone3"
      }
    }
  },
  ChiCon050 = {
    sModuleName = "ChiCon050",
    sFactionId = "Chi",
    sStarter = "ChiStarter1",
    sTitle = "[ChiCon050.Title]",
    bCriticalPathMission = true
  },
  ChiCon051 = {
    sModuleName = "ChiCon051",
    sFactionId = "Chi",
    sStarter = "ChiStarter2",
    sTitle = "[ChiCon051.Title]"
  },
  ChiCon053 = {
    sModuleName = "ChiCon053",
    sFactionId = "Chi",
    sStarter = "ChiStarter3",
    sTitle = "[ChiCon053.Title]"
  },
  ChiJob002 = {
    sModuleName = "ChiJob002",
    sFactionId = "Chi",
    tMilestones = {
      {
        nMilestone = 1,
        sKey = "ChiJob002_Milestone1"
      },
      {
        nMilestone = 2,
        sKey = "ChiJob002_Milestone2"
      },
      {
        nMilestone = 3,
        sKey = "ChiJob002_Milestone3"
      },
      {
        nMilestone = 4,
        sKey = "ChiJob002_Milestone4"
      },
      {
        nMilestone = 5,
        sKey = "ChiJob002_Milestone5"
      },
      {
        nMilestone = 6,
        sKey = "ChiJob002_Milestone6"
      },
      {
        nMilestone = 7,
        sKey = "ChiJob002_Milestone7"
      },
      {
        nMilestone = 8,
        sKey = "ChiJob002_Milestone8"
      },
      {
        nMilestone = 9,
        sKey = "ChiJob002_Milestone9"
      },
      {
        nMilestone = 10,
        sKey = "ChiJob002_Milestone10"
      }
    },
    sPdaTexture = "icon_verify_2_mc",
    nPdaSortOrder = knPdaSortOrderVerifySet
  },
  ChiJob003 = {
    sModuleName = "ChiJob003",
    sFactionId = "Chi",
    bCompletable = false,
    tMilestones = {
      {
        nMilestone = 3,
        sKey = "ChiJob003_Milestone1"
      },
      {
        nMilestone = 10,
        sKey = "ChiJob003_Milestone2"
      },
      {
        nMilestone = 25,
        sKey = "ChiJob003_Milestone3"
      },
      {
        nMilestone = 50,
        sKey = "ChiJob003_Milestone4"
      }
    },
    sPdaTexture = "icon_destroy_2_mc",
    nPdaSortOrder = knPdaSortOrderStandingBty
  },
  ChiJob020 = {
    sModuleName = "ChiJob020",
    sFactionId = "Chi",
    tMilestones = {
      {
        nMilestone = 1,
        sKey = "ChiJob020_Milestone1"
      },
      {
        nMilestone = 3,
        sKey = "ChiJob020_Milestone2"
      },
      {
        nMilestone = 5,
        sKey = "ChiJob020_Milestone3"
      },
      {
        nMilestone = 8,
        sKey = "ChiJob020_Milestone4"
      }
    },
    sPdaTexture = "icon_destroy_2_mc",
    nPdaSortOrder = knPdaSortOrderDestroySet
  },
  GurCon001 = {
    sModuleName = "GurCon001",
    sFactionId = "Gur",
    tMaterielScale = {Vza = 1},
    sStarter = "GurStarter0",
    bCriticalPathMission = true
  },
  GurCon002 = {
    sModuleName = "GurCon002",
    sFactionId = "Gur",
    tMaterielScale = {Vza = 1},
    sStarter = "GurStarter0",
    bCriticalPathMission = true
  },
  GurCon003 = {
    sModuleName = "GurCon003",
    sFactionId = "Gur",
    sStarter = "GurStarter5",
    tMaterielScale = {Vza = 1},
    bRepeatable = true,
    nLevels = 3,
    bCriticalPathMission = true,
    tMilestones = {
      {
        nMilestone = 1,
        sKey = "GurCon003_Milestone1"
      },
      {
        nMilestone = 2,
        sKey = "GurCon003_Milestone2"
      },
      {
        nMilestone = 3,
        sKey = "GurCon003_Milestone3"
      }
    }
  },
  GurCon005 = {
    sModuleName = "GurCon005",
    sFactionId = "Gur",
    sStarter = "GurStarter2",
    tMaterielScale = {Vza = 1}
  },
  GurCon050 = {
    sModuleName = "GurCon050",
    sFactionId = "Gur",
    sStarter = "GurStarter5",
    sTitle = "[GurCon050.Title]"
  },
  GurCon052 = {
    sModuleName = "GurCon052",
    sFactionId = "Gur",
    sStarter = "GurStarter2",
    sTitle = "[GurCon052.Title]"
  },
  GurCon053 = {
    sModuleName = "GurCon053",
    sFactionId = "Gur",
    sStarter = "GurStarter1",
    sTitle = "[GurCon053.Title]",
    bCriticalPathMission = true
  },
  GurJob001 = {
    sModuleName = "GurJob001",
    sFactionId = "Gur",
    bCompleteable = false,
    tMilestones = {
      {
        nMilestone = 1,
        sKey = "GurJob001_Milestone1"
      },
      {
        nMilestone = 5,
        sKey = "GurJob001_Milestone2"
      },
      {
        nMilestone = 10,
        sKey = "GurJob001_Milestone3"
      },
      {
        nMilestone = 20,
        sKey = "GurJob001_Milestone4"
      },
      {
        nMilestone = 40,
        sKey = "GurJob001_Milestone5"
      }
    },
    sPdaTexture = "icon_destroy_2_mc",
    nPdaSortOrder = knPdaSortOrderBillboards
  },
  GurJob002 = {
    sModuleName = "GurJob002",
    sFactionId = "Gur",
    tMilestones = {
      {
        nMilestone = 1,
        sKey = "GurJob002_Milestone1"
      },
      {
        nMilestone = 2,
        sKey = "GurJob002_Milestone2"
      },
      {
        nMilestone = 3,
        sKey = "GurJob002_Milestone3"
      },
      {
        nMilestone = 4,
        sKey = "GurJob002_Milestone4"
      },
      {
        nMilestone = 5,
        sKey = "GurJob002_Milestone5"
      },
      {
        nMilestone = 6,
        sKey = "GurJob002_Milestone6"
      },
      {
        nMilestone = 7,
        sKey = "GurJob002_Milestone7"
      },
      {
        nMilestone = 8,
        sKey = "GurJob002_Milestone8"
      },
      {
        nMilestone = 9,
        sKey = "GurJob002_Milestone8"
      },
      {
        nMilestone = 10,
        sKey = "GurJob002_Milestone10"
      }
    },
    sPdaTexture = "icon_verify_2_mc",
    nPdaSortOrder = knPdaSortOrderVerifySet
  },
  GurJob006 = {
    sModuleName = "GurJob006",
    sFactionId = "Gur",
    bCompleteable = false,
    tMilestones = {
      {
        nMilestone = 3,
        sKey = "GurJob006_Milestone1"
      },
      {
        nMilestone = 10,
        sKey = "GurJob006_Milestone2"
      },
      {
        nMilestone = 25,
        sKey = "GurJob006_Milestone3"
      },
      {
        nMilestone = 50,
        sKey = "GurJob006_Milestone4"
      }
    },
    sPdaTexture = "icon_destroy_2_mc",
    nPdaSortOrder = knPdaSortOrderStandingBty
  },
  GurJob020 = {
    sModuleName = "GurJob020",
    sFactionId = "Gur",
    tMilestones = {
      {
        nMilestone = 1,
        sKey = "GurJob020_Milestone1"
      },
      {
        nMilestone = 3,
        sKey = "GurJob020_Milestone2"
      },
      {
        nMilestone = 5,
        sKey = "GurJob020_Milestone3"
      },
      {
        nMilestone = 8,
        sKey = "GurJob020_Milestone4"
      },
      {
        nMilestone = 13,
        sKey = "GurJob020_Milestone5"
      }
    },
    sPdaTexture = "icon_destroy_2_mc",
    nPdaSortOrder = knPdaSortOrderDestroySet
  },
  OilCon021 = {
    sModuleName = "OilCon021",
    sFactionId = "Oil",
    tMaterielScale = {Vza = 1},
    sStarter = "OilStarter5",
    bCriticalPathMission = true
  },
  OilCon001 = {
    sModuleName = "OilCon001",
    sFactionId = "Oil",
    tMaterielScale = {Vza = 1},
    sStarter = "OilStarter0",
    bCriticalPathMission = true
  },
  OilCon002 = {
    sModuleName = "OilCon002",
    sFactionId = "Oil",
    tMaterielScale = {Vza = 1},
    tLayers = {
      "Vz_State_OilCon002"
    },
    sStarter = "OilStarter0",
    bCriticalPathMission = true
  },
  OilCon003 = {
    sModuleName = "OilCon003",
    sFactionId = "Oil",
    tMaterielScale = {Vza = 1},
    sStarter = "OilStarter3",
    bRepeatable = true,
    nLevels = 3,
    tMilestones = {
      {
        nMilestone = 1,
        sKey = "OilCon003_Milestone1"
      },
      {
        nMilestone = 2,
        sKey = "OilCon003_Milestone2"
      },
      {
        nMilestone = 3,
        sKey = "OilCon003_Milestone3"
      }
    }
  },
  OilCon005 = {
    sModuleName = "OilCon005",
    sFactionId = "Oil",
    sStarter = "OilStarter4",
    tStartLocations = {
      "OilCon005_Startpoint_01",
      "OilCon005_Startpoint_02"
    },
    tMaterielScale = {Vza = 1},
    bRepeatable = true,
    nLevels = 3,
    tMilestones = {
      {
        nMilestone = 1,
        sKey = "OilCon005_Milestone1"
      },
      {
        nMilestone = 2,
        sKey = "OilCon005_Milestone2"
      },
      {
        nMilestone = 3,
        sKey = "OilCon005_Milestone3"
      }
    }
  },
  OilCon050 = {
    sModuleName = "OilCon050",
    sFactionId = "Oil",
    sStarter = "OilStarter1",
    sTitle = "[OilCon050.Title]",
    bCriticalPathMission = true
  },
  OilCon051 = {
    sModuleName = "OilCon051",
    sFactionId = "Oil",
    sStarter = "OilStarter2",
    sTitle = "[OilCon051.Title]"
  },
  OilCon052 = {
    sModuleName = "OilCon052",
    sFactionId = "Oil",
    sStarter = "OilStarter3",
    sTitle = "[OilCon052.Title]"
  },
  OilJob004 = {
    sModuleName = "OilJob004",
    sFactionId = "Oil",
    bCompletable = false,
    tMilestones = {
      {
        nMilestone = 3,
        sKey = "OilJob004_Milestone1"
      },
      {
        nMilestone = 10,
        sKey = "OilJob004_Milestone2"
      },
      {
        nMilestone = 25,
        sKey = "OilJob004_Milestone3"
      },
      {
        nMilestone = 50,
        sKey = "OilJob004_Milestone4"
      }
    },
    sPdaTexture = "icon_destroy_2_mc",
    nPdaSortOrder = knPdaSortOrderStandingBty
  },
  OilJob008 = {
    sModuleName = "OilJob008",
    sFactionId = "Oil",
    tMilestones = {
      {
        nMilestone = 1,
        sKey = "OilJob008_Milestone1"
      },
      {
        nMilestone = 3,
        sKey = "OilJob008_Milestone2"
      },
      {
        nMilestone = 5,
        sKey = "OilJob008_Milestone3"
      },
      {
        nMilestone = 8,
        sKey = "OilJob008_Milestone4"
      },
      {
        nMilestone = 13,
        sKey = "OilJob008_Milestone5"
      }
    },
    sPdaTexture = "icon_destroy_2_mc",
    nPdaSortOrder = knPdaSortOrderDestroySet
  },
  OilJob011 = {
    sModuleName = "OilJob011",
    sFactionId = "Oil",
    tMilestones = {
      {
        nMilestone = 1,
        sKey = "OilJob011_Milestone1"
      },
      {
        nMilestone = 2,
        sKey = "OilJob011_Milestone2"
      },
      {
        nMilestone = 3,
        sKey = "OilJob011_Milestone3"
      },
      {
        nMilestone = 4,
        sKey = "OilJob011_Milestone4"
      },
      {
        nMilestone = 5,
        sKey = "OilJob011_Milestone5"
      },
      {
        nMilestone = 6,
        sKey = "OilJob011_Milestone6"
      },
      {
        nMilestone = 7,
        sKey = "OilJob011_Milestone7"
      },
      {
        nMilestone = 8,
        sKey = "OilJob011_Milestone8"
      },
      {
        nMilestone = 9,
        sKey = "OilJob011_Milestone9"
      },
      {
        nMilestone = 10,
        sKey = "OilJob011_Milestone10"
      }
    },
    sPdaTexture = "icon_verify_2_mc",
    nPdaSortOrder = knPdaSortOrderVerifySet
  },
  VzaCon001 = {
    sModuleName = "VzaCon001",
    sFactionId = "Vza",
    tStartLocations = {
      "VzaCon001_Start1"
    },
    bPlayerVisibleMission = true,
    bCriticalPathMission = true
  },
  OilCon020 = {
    sModuleName = "OilCon020",
    sStarter = "PmcBoss",
    sFactionId = "Pmc",
    tMaterielScale = {Vza = 1},
    tLayers = {
      "Vz_State_OilCon020",
      "VZ_State_OilCon020_Deliveribles"
    },
    bCriticalPathMission = true
  },
  PmcCon001 = {
    sModuleName = "PmcCon001",
    sFactionId = "Pmc",
    tStartLocations = {
      "PmcCon001_Start1"
    },
    bPlayerVisibleMission = true,
    bCriticalPathMission = true
  },
  PmcCon002 = {
    sModuleName = "PmcCon002",
    tStartLocations = {"Pmc_B1", "Pmc_B2"},
    sFactionId = "Pmc",
    sStarter = "PmcBoss",
    tLayers = {
      "vz_state_pmccon002",
      "vz_state_mer_oilrig_pristine",
      "vz_state_mer_oilrig_pristine_tg"
    },
    bPlayerVisibleMission = true,
    bCriticalPathMission = true
  },
  PmcCon003 = {
    sModuleName = "PmcCon003",
    sFactionId = "Pmc",
    sStarter = "PmcBoss",
    tLayers = {
      "vz_state_pmccon003",
      "vz_state_PMCCon003_pristine",
      "vz_state_sol_base_pristine"
    },
    bPlayerVisibleMission = true,
    bCriticalPathMission = true
  },
  PmcCon004 = {
    sModuleName = "PmcCon004",
    sFactionId = "Pmc",
    tStartLocations = {
      "PmcCon004_Start1",
      "PmcCon004_Start2"
    },
    bPlayerVisibleMission = true,
    bCriticalPathMission = true
  },
  PmcCon031 = {
    sModuleName = "PmcCon031",
    sFactionId = "Pmc",
    sStarter = "PmcBoss",
    tStartLocations = {
      "PMCCon031_Start1",
      "PMCCon031_Start2"
    },
    bRepeatable = true,
    nLevels = 3,
    tMilestones = {
      {
        nMilestone = 2,
        sKey = "PmcCon031_Milestone1"
      }
    }
  },
  PmcCon032 = {
    sModuleName = "PmcCon032",
    sFactionId = "Pmc",
    sStarter = "PmcBoss",
    tStartLocations = {
      "PMCCon032_Start1",
      "PMCCon032_Start2"
    },
    bRepeatable = true,
    nLevels = 3,
    tMilestones = {
      {
        nMilestone = 2,
        sKey = "PmcCon032_Milestone1"
      }
    }
  },
  PmcCon033 = {
    sModuleName = "PmcCon033",
    sFactionId = "Pmc",
    sStarter = "PmcBoss",
    tStartLocations = {
      "PMCCon033_Start1",
      "PMCCon033_Start2"
    },
    bRepeatable = true,
    nLevels = 3,
    tMilestones = {
      {
        nMilestone = 2,
        sKey = "PmcCon033_Milestone1"
      }
    }
  },
  PmcCon034 = {
    sModuleName = "PmcCon034",
    sFactionId = "Pmc",
    sStarter = "PmcBoss",
    tStartLocations = {
      "PMCCon034_Start1",
      "PMCCon034_Start2"
    },
    bRepeatable = true,
    nLevels = 3,
    tMilestones = {
      {
        nMilestone = 2,
        sKey = "PmcCon034_Milestone1"
      }
    }
  },
  PmcCon013 = {
    sModuleName = "PmcCon013",
    sFactionId = "Pmc",
    sStarter = "HelPmcBoss",
    tLayers = {
      "vz_state_PmcCon013_MP",
      "vz_state_PmcCon013"
    },
    bRepeatable = true,
    nLevels = 3
  },
  PmcCon015 = {
    sModuleName = "PmcCon015",
    sFactionId = "Pmc",
    sStarter = "MecPmcBoss",
    bRepeatable = true,
    nLevels = 3,
    tMilestones = {
      {
        nMilestone = 1,
        sKey = "PmcCon015_Milestone1"
      },
      {
        nMilestone = 2,
        sKey = "PmcCon015_Milestone2"
      },
      {
        nMilestone = 3,
        sKey = "PmcCon015_Milestone3"
      }
    }
  },
  PmcCon016 = {
    sModuleName = "PmcCon016",
    sFactionId = "Pmc",
    sStarter = "MecPmcBoss",
    bRepeatable = true,
    nLevels = 3,
    tMilestones = {
      {
        nMilestone = 1,
        sKey = "PmcCon016_Milestone1"
      },
      {
        nMilestone = 2,
        sKey = "PmcCon016_Milestone2"
      },
      {
        nMilestone = 3,
        sKey = "PmcCon016_Milestone3"
      }
    }
  },
  PmcCon018 = {
    sModuleName = "PmcCon018",
    sFactionId = "Pmc",
    tMaterielScale = {Vza = 1},
    sStarter = "JetPmcBoss",
    tStartLocations = {
      "PMCCon018_Start1",
      "PMCCon018_Start2"
    },
    tLayers = {
      "vz_State_PmcCon018_Veh",
      "vz_State_PmcCon018"
    },
    bRepeatable = true,
    nLevels = 3,
    tMilestones = {
      {
        nMilestone = 1,
        sKey = "PmcCon018_Milestone1"
      },
      {
        nMilestone = 2,
        sKey = "PmcCon018_Milestone2"
      },
      {
        nMilestone = 3,
        sKey = "PmcCon018_Milestone3"
      }
    }
  },
  PmcJob001 = {
    sModuleName = "PmcJob001",
    sFactionId = "Pmc",
    bSkipInitialNotifications = true,
    bSuppressPdaDisplay = true,
    tMilestones = {
      {
        nMilestone = 10,
        sKey = "PmcJob001_Milestone1"
      },
      {
        nMilestone = 20,
        sKey = "PmcJob001_Milestone2"
      },
      {
        nMilestone = 30,
        sKey = "PmcJob001_Milestone3"
      },
      {
        nMilestone = 40,
        sKey = "PmcJob001_Milestone4"
      },
      {
        nMilestone = 50,
        sKey = "PmcJob001_Milestone5"
      },
      {
        nMilestone = 60,
        sKey = "PmcJob001_Milestone6"
      },
      {
        nMilestone = 70,
        sKey = "PmcJob001_Milestone7"
      },
      {
        nMilestone = 85,
        sKey = "PmcJob001_Milestone8"
      },
      {
        nMilestone = 100,
        sKey = "PmcJob001_Milestone9"
      }
    }
  },
  PmcJob002 = {
    sModuleName = "PmcJob002",
    sFactionId = "Pmc",
    bSkipInitialNotifications = true,
    bSuppressPdaDisplay = true,
    tMilestones = {
      {
        nMilestone = 10,
        sKey = "PmcJob002_Milestone1"
      },
      {
        nMilestone = 25,
        sKey = "PmcJob002_Milestone2"
      },
      {
        nMilestone = 50,
        sKey = "PmcJob002_Milestone3"
      }
    }
  },
  MecCon001 = {
    sModuleName = "MecCon001",
    sFactionId = "Pmc",
    sStarter = "MecBoss",
    bPlayerVisibleMission = true,
    bCriticalPathMission = true
  },
  JetCon001 = {
    sModuleName = "JetCon001",
    sFactionId = "Pmc",
    sStarter = "JetBoss",
    tLayers = {
      "vz_state_JetCon001",
      "vz_state_JetCon001_pristine"
    },
    bPlayerVisibleMission = true,
    bCriticalPathMission = true
  },
  PirCon001 = {
    sModuleName = "PirCon001",
    sFactionId = "Pir",
    sStarter = "PirStarter1",
    tStartLocations = {
      "PirCon001_Startpoint_01",
      "PirCon001_Startpoint_02"
    },
    bRepeatable = true,
    nLevels = 3,
    tMilestones = {
      {
        nMilestone = 1,
        sKey = "PirCon001_Milestone1"
      },
      {
        nMilestone = 2,
        sKey = "PirCon001_Milestone2"
      },
      {
        nMilestone = 3,
        sKey = "PirCon001_Milestone3"
      }
    }
  },
  PirCon002 = {
    sModuleName = "PirCon002",
    sFactionId = "Pir",
    sStarter = "PirStarter1",
    bRepeatable = true,
    nLevels = 3,
    tLayers = {
      "vz_state_PirCon002",
      "vz_state_PirCon002_Deliverables"
    },
    tMilestones = {
      {
        nMilestone = 1,
        sKey = "PirCon002_Milestone1"
      },
      {
        nMilestone = 2,
        sKey = "PirCon002_Milestone2"
      },
      {
        nMilestone = 3,
        sKey = "PirCon002_Milestone3"
      }
    }
  },
  PirCon003 = {
    sModuleName = "PirCon003",
    sFactionId = "Pir",
    sStarter = "PirStarter3",
    bRepeatable = true,
    nLevels = 3,
    tLayers = {
      "vz_state_PirCon003",
      "vz_state_PirCon003_Deliverables"
    },
    tMilestones = {
      {
        nMilestone = 1,
        sKey = "PirCon003_Milestone1"
      },
      {
        nMilestone = 2,
        sKey = "PirCon003_Milestone2"
      },
      {
        nMilestone = 3,
        sKey = "PirCon003_Milestone3"
      }
    }
  },
  PirCon004 = {
    sModuleName = "PirCon004",
    sFactionId = "Pir",
    sStarter = "PirStarter4",
    bRepeatable = true,
    nLevels = 3,
    tMilestones = {
      {
        nMilestone = 1,
        sKey = "PirCon004_Milestone1"
      },
      {
        nMilestone = 2,
        sKey = "PirCon004_Milestone2"
      },
      {
        nMilestone = 3,
        sKey = "PirCon004_Milestone3"
      }
    }
  },
  PirCon051 = {
    sModuleName = "PirCon051",
    sFactionId = "Pir",
    sStarter = "PirStarter1",
    sTitle = "[PirCon051.Title]"
  },
  PirCon052 = {
    sModuleName = "PirCon052",
    sFactionId = "Pir",
    sStarter = "PirStarter3",
    sTitle = "[PirCon052.Title]"
  },
  PirJob012 = {
    sModuleName = "PirJob012",
    sFactionId = "Pir",
    tMilestones = {
      {
        nMilestone = 1,
        sKey = "PirJob012_Milestone1"
      },
      {
        nMilestone = 2,
        sKey = "PirJob012_Milestone2"
      },
      {
        nMilestone = 3,
        sKey = "PirJob012_Milestone3"
      },
      {
        nMilestone = 4,
        sKey = "PirJob012_Milestone4"
      },
      {
        nMilestone = 5,
        sKey = "PirJob012_Milestone5"
      },
      {
        nMilestone = 6,
        sKey = "PirJob012_Milestone6"
      },
      {
        nMilestone = 7,
        sKey = "PirJob012_Milestone7"
      },
      {
        nMilestone = 8,
        sKey = "PirJob012_Milestone8"
      },
      {
        nMilestone = 9,
        sKey = "PirJob012_Milestone9"
      },
      {
        nMilestone = 10,
        sKey = "PirJob012_Milestone10"
      }
    },
    sPdaTexture = "icon_verify_2_mc",
    nPdaSortOrder = knPdaSortOrderVerifySet
  },
  PirJob020 = {
    sModuleName = "PirJob020",
    sFactionId = "Pir",
    tMilestones = {
      {
        nMilestone = 1,
        sKey = "PirJob020_Milestone1"
      },
      {
        nMilestone = 3,
        sKey = "PirJob020_Milestone2"
      },
      {
        nMilestone = 5,
        sKey = "PirJob020_Milestone3"
      },
      {
        nMilestone = 8,
        sKey = "PirJob020_Milestone4"
      },
      {
        nMilestone = 11,
        sKey = "PirJob020_Milestone5"
      }
    },
    sPdaTexture = "icon_destroy_2_mc",
    nPdaSortOrder = knPdaSortOrderDestroySet
  }
}
import("MrxUtil")
import("WifMissionFlow")

function Init()
  for sMissionId, tMissionConfig in pairs(tMissionData) do
    local sFaction, bMissionType, nMissionNum = MrxUtil.ExplodeMissionName(sMissionId)
    tMissionConfig.bContract = bMissionType
  end
end

function IsMissionAContract(sMissionId)
  if tMissionData[sMissionId] then
    return tMissionData[sMissionId].bContract == true
  end
end

function IsMissionAJob(sMissionId)
  if tMissionData[sMissionId] then
    return tMissionData[sMissionId].bContract == false
  end
end

function IsMissionOnCriticalPath(sMissionId)
  if tMissionData[sMissionId] then
    return tMissionData[sMissionId].bCriticalPathMission == true
  end
end

function GetMissionTitle(sMissionId)
  return "[" .. sMissionId .. ".Title]"
end

function GetMissionFaction(sMissionId)
  return tMissionData[sMissionId].sFactionId
end

function GetMissionIndexFromId(sMissionId)
  local index = 1
  for _id, tMissionConfig in pairs(tMissionData) do
    if _id == sMissionId then
      return index
    end
    index = index + 1
  end
  return nil
end

function GetMissionIdFromIndex(nMissionIndex)
  local index = 1
  for _id, tMissionConfig in pairs(tMissionData) do
    if index == nMissionIndex then
      return _id
    end
    index = index + 1
  end
  return nil
end

function GetMissionStarter(sMissionId)
  return tMissionData[sMissionId].sStarter
end

function SetMissionData(tNewMissionData)
  if tNewMissionData == nil then
    return nil
  end
  tMissionData = nil
  tMissionData = tNewMissionData
  return true
end

function GetMissionMilestoneData(sMissionId)
  if sMissionId and tMissionData[sMissionId] then
    return tMissionData[sMissionId].tMilestones
  end
end

function GetIsCompleteable(sMissionId)
  local retVal = tMissionData[sMissionId].bCompleteable
  if retVal == nil then
    retVal = true
  end
  return retVal
end

function GetMissionRepeatable(sMissionId)
  if sMissionId and tMissionData[sMissionId] then
    return tMissionData[sMissionId].bRepeatable
  end
end

function GetMissionLevels(sMissionId)
  if sMissionId and tMissionData[sMissionId] then
    return tMissionData[sMissionId].nLevels
  end
end

function IsMissionSuppressedInPda(sMissionId)
  if sMissionId and tMissionData[sMissionId] then
    return tMissionData[sMissionId].bSuppressPdaDisplay
  end
end

function GetMissionPdaTexture(sMissionId)
  if sMissionId and tMissionData[sMissionId] then
    return tMissionData[sMissionId].sPdaTexture
  end
end

function GetPdaSortOrder(sMissionId)
  if sMissionId and tMissionData[sMissionId] then
    return tMissionData[sMissionId].nPdaSortOrder
  end
end

function GetNumCompletedContracts()
  local tStates = WifMissionFlow.GetMissionStates()
  local nContracts = 0
  for i, v in pairs(tStates) do
    if IsMissionAContract(v[1]) and v[2] == "complete" then
      nContracts = nContracts + 1
    end
  end
  return nContracts
end

function GetNumContracts()
  local nContracts = 0
  for sMissionId, _ in pairs(tMissionData) do
    if IsMissionAContract(sMissionId) then
      nContracts = nContracts + 1
    end
  end
  return nContracts
end
