knContact = 1
knSimple = 2
knRecruit = 3
Intros = {
  Gur = {
    tHq = {
      "GurOutpost1"
    },
    sTitle = "[Briefing.Intro.Gur]",
    tSequence = {
      {
        sSpeaker = "Starter",
        sCue = "Fiona.Guerrilla.Intro01"
      },
      {
        sFlashFile = "POI_GR_Introduction",
        nTime = 35
      },
      0.1,
      {
        sSpeaker = "Starter",
        sCue = "Fiona.Guerrilla.Intro02"
      },
      {
        sSpeaker = "Starter",
        sCue = "Fiona.Guerrilla.Intro03"
      },
      {
        sSpeaker = "Starter",
        sCue = "Fiona.Guerrilla.Intro04"
      },
      {
        sSpeaker = "Player1",
        sCue = {
          Mattias = "Mattias.Guerrilla01",
          Jennifer = "Jen.Guerrilla01",
          Chris = "Chris.Guerrilla01"
        }
      }
    }
  },
  Pir = {
    tHq = {
      "PirOutpost1"
    },
    sTitle = "[Briefing.Intro.Pir]",
    tSequence = {
      {
        sSpeaker = "Starter",
        sCue = "Fiona.Pirates.Intro01"
      },
      {
        sFlashFile = "POI_PR_Introduction",
        nTime = 35
      },
      0.1,
      {
        sSpeaker = "Starter",
        sCue = "Fiona.Pirates.Intro02"
      },
      {
        sSpeaker = "Starter",
        sCue = "Fiona.Pirates.Intro03"
      },
      {
        sSpeaker = "Starter",
        sCue = "Fiona.Pirates.Intro04"
      }
    }
  },
  AllChi = {
    tHq = {
      "ChiOutpost1",
      "AllOutpost1"
    },
    sTitle = "[Briefing.Intro.AllChi]",
    tSequence = {
      {
        sSpeaker = "Starter",
        sCue = "Fiona.All_Chi.Intro01"
      },
      {
        sFlashFile = "POI_ANCH_Introduction",
        nTime = 50
      },
      0.1,
      {
        sSpeaker = "Player1",
        sCue = {
          Mattias = "Mattias.All_Chi01",
          Jennifer = "Jen.All_Chi01",
          Chris = "Chris.All_Chi01"
        }
      },
      {
        sSpeaker = "Starter",
        sCue = "Fiona.All_Chi.Intro02"
      },
      {
        sSpeaker = "Player1",
        sCue = {
          Mattias = "Mattias.All_Chi02",
          Jennifer = "Jen.All_Chi02",
          Chris = "Chris.All_Chi02"
        }
      },
      {
        sSpeaker = "Starter",
        sCue = "Fiona.All_Chi.Intro03"
      },
      {
        sSpeaker = "Player1",
        sCue = {
          Mattias = "Mattias.All_Chi03",
          Jennifer = "Jen.All_Chi03",
          Chris = "Chris.All_Chi03"
        }
      },
      {
        sSpeaker = "Starter",
        sCue = "Fiona.All_Chi.Intro04"
      }
    }
  },
  Mec = {
    tHq = {"MecHq"},
    sTitle = "[Briefing.Intro.MecPmcBoss]",
    tSequence = {
      {
        sSpeaker = "Starter",
        sCue = "Ewan.PMC.RecruitEva01"
      }
    }
  },
  Jet = {
    tHq = {"JetHq"},
    sTitle = "[Briefing.Intro.JetPmcBoss]",
    tSequence = {
      {
        sSpeaker = "Starter",
        sCue = "Eva.PMC.RecruitMisha01"
      }
    }
  }
}

function GetIntroIdByIndex(nIntroIndex)
  local index = 1
  for _id, tData in pairs(Intros) do
    if index == nIntroIndex then
      return _id
    end
    index = index + 1
  end
  return nil
end

function GetIntroIndexById(sId)
  local index = 1
  for _id, tData in pairs(Intros) do
    if _id == sId then
      return index
    end
    index = index + 1
  end
  return nil
end

AllCon001 = {
  tAssetPreload = {
    soundbank = {
      "vo_allCon001"
    },
    wavebank = {
      "vo_allCon001"
    }
  },
  tActors = {
    HeroChair = {
      sTemplate = "_aloutpost_interior_herochair",
      sPosition = "hp_al01_player"
    }
  },
  tPositions = {
    Player1 = "hp_al01_player",
    Starter = "hp_al01_starter"
  },
  tFaceAnimSets = {
    Player1 = {
      Chris = "ALL01_Contract_Briefing_Chris",
      Jennifer = "ALL01_Contract_Briefing_Jennifer",
      Mattias = "ALL01_Contract_Briefing_Mattias"
    },
    Starter = "ALL01_Contract_Briefing_Starter"
  },
  tCinematic = {
    Chris = {
      {
        tAnims = {
          Player1 = "ALL01_Contract_Briefing_Chris",
          Starter = "ALL01_Contract_Briefing_Starter-Chris",
          HeroChair = "ALL01_Contract_Briefing_HeroChair-Chris"
        },
        OnTime = 32.1333,
        OnComplete = "Player1"
      },
      {
        tFlash = {
          sFile = "AllCon001_briefing.gfx",
          nTime = 29.1
        },
        OnTime = 39.566696
      },
      {
        tCamera = {bHold = true},
        Stall = true
      }
    },
    Jennifer = {
      {
        tAnims = {
          Player1 = "ALL01_Contract_Briefing_Jennifer",
          Starter = "ALL01_Contract_Briefing_Starter-Jennifer",
          HeroChair = "ALL01_Contract_Briefing_HeroChair-Jennifer"
        },
        OnTime = 34.1333,
        OnComplete = "Player1"
      },
      {
        tFlash = {
          sFile = "AllCon001_briefing.gfx",
          nTime = 32.3
        },
        OnTime = 42.933403
      },
      {
        tCamera = {bHold = true},
        Stall = true
      }
    },
    Mattias = {
      {
        tAnims = {
          Player1 = "ALL01_Contract_Briefing_Mattias",
          Starter = "ALL01_Contract_Briefing_Starter-Mattias",
          HeroChair = "ALL01_Contract_Briefing_HeroChair-Mattias"
        },
        OnTime = 33.1333,
        OnComplete = "Player1"
      },
      {
        tFlash = {
          sFile = "AllCon001_briefing.gfx",
          nTime = 35.5333
        },
        OnTime = 46.8667
      },
      {
        tCamera = {bHold = true},
        Stall = true
      }
    },
    tCameraEffects = {
      Chris = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.096,
            2.9907,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 2.27,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.096,
            6.7799,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 3.63,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.096,
            2.5085,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 6.1,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.096,
            2.5085,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 9.7,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.096,
            4.6838,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 12.2,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.096,
            2.1867,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 19,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.096,
            5.626,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            93.212,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 25.53,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.096,
            2.6113,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 64.37,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.096,
            1.4029,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 69.13,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.096,
            1.4029,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 72.83,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.096,
            5.1246,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Jennifer = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.096,
            2.9907,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 2.53,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.096,
            6.7934,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 3.77,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.096,
            2.5085,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 5.37,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.096,
            2.5085,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 12.7,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.096,
            4.6838,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            60,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 15.6,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.096,
            2.1867,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            60,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 22.4,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.096,
            5.626,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            90,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 29.13,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.096,
            2.6113,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 70.73,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.096,
            1.4029,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 74.57,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.096,
            1.4029,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Mattias = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.096,
            2.9907,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 2.53,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.096,
            4.971,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 4,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.096,
            2.5085,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 6.1,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.096,
            2.5085,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 12.7,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.096,
            4.6838,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 15.2,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.096,
            1.628,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 21.9,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.096,
            5.626,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            93.212,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 28.43,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.096,
            1.9533,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 72.4,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.096,
            1.4029,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 77.37,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.096,
            1.4029,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      }
    }
  },
  tDeclineCinematic = {
    Chris = {
      {
        tAnims = {
          Player1 = "ALL01_Contract_Briefing_No_Chris",
          Starter = "ALL01_Contract_Briefing_No_Starter-Chris",
          HeroChair = "ALL01_Contract_Briefing_No_HeroChair-Chris"
        },
        OnComplete = "Player1"
      }
    },
    Jennifer = {
      {
        tAnims = {
          Player1 = "ALL01_Contract_Briefing_No_Jennifer",
          Starter = "ALL01_Contract_Briefing_No_Starter-Jennifer",
          HeroChair = "ALL01_Contract_Briefing_No_HeroChair-Jennifer"
        },
        OnComplete = "Player1"
      }
    },
    Mattias = {
      {
        tAnims = {
          Player1 = "ALL01_Contract_Briefing_No_Mattias",
          Starter = "ALL01_Contract_Briefing_No_Starter-Mattias",
          HeroChair = "ALL01_Contract_Briefing_No_HeroChair-Mattias"
        },
        OnComplete = "Player1"
      }
    },
    tCameraEffects = {
      Chris = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.096,
            5.1246,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Jennifer = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.096,
            4.3111,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Mattias = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.096,
            4.8841,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      }
    }
  },
  tConfirmCinematic = {
    Chris = {
      {
        tAnims = {
          Player1 = "ALL01_Contract_Briefing_Yes_Chris",
          Starter = "ALL01_Contract_Briefing_Yes_Starter-Chris",
          HeroChair = "ALL01_Contract_Briefing_No_HeroChair-Chris"
        },
        OnComplete = "Player1"
      }
    },
    Jennifer = {
      {
        tAnims = {
          Player1 = "ALL01_Contract_Briefing_Yes_Jennifer",
          Starter = "ALL01_Contract_Briefing_Yes_Starter-Jennifer",
          HeroChair = "ALL01_Contract_Briefing_No_HeroChair-Jennifer"
        },
        OnComplete = "Player1"
      }
    },
    Mattias = {
      {
        tAnims = {
          Player1 = "ALL01_Contract_Briefing_Yes_Mattias",
          Starter = "ALL01_Contract_Briefing_Yes_Starter-Mattias",
          HeroChair = "ALL01_Contract_Briefing_No_HeroChair-Mattias"
        },
        OnComplete = "Player1"
      }
    },
    tCameraEffects = {
      Chris = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.096,
            5.1246,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Jennifer = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.096,
            4.3111,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Mattias = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.096,
            4.8841,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      }
    }
  }
}
AllCon002 = {
  tAssetPreload = {
    soundbank = {
      "vo_allCon002"
    },
    wavebank = {
      "vo_allCon002"
    }
  },
  tPositions = {
    Player1 = "hp_al02_player",
    Starter = "hp_al02_starter"
  },
  tFaceAnimSets = {
    Player1 = {
      Chris = "ALL02_Contract_Briefing_Chris",
      Jennifer = "ALL02_Contract_Briefing_Jennifer",
      Mattias = "ALL02_Contract_Briefing_Mattias"
    },
    Starter = "ALL02_Contract_Briefing_Starter"
  },
  tCinematic = {
    Chris = {
      {
        tAnims = {
          Player1 = "ALL02_Contract_Briefing_Chris",
          Starter = "ALL02_Contract_Briefing_Starter-Chris"
        },
        OnTime = 92.0667,
        OnComplete = "Player1"
      },
      {
        tFlash = {
          sFile = "AllCon002_briefing.gfx",
          nTime = 34.1
        },
        OnTime = 60.133293
      },
      {
        tCamera = {bHold = true},
        Stall = true
      }
    },
    Jennifer = {
      {
        tAnims = {
          Player1 = "ALL02_Contract_Briefing_Jennifer",
          Starter = "ALL02_Contract_Briefing_Starter-Jennifer"
        },
        OnTime = 87.7,
        OnComplete = "Player1"
      },
      {
        tFlash = {
          sFile = "AllCon002_briefing.gfx",
          nTime = 33.6333
        },
        OnTime = 60.300003
      },
      {
        tCamera = {bHold = true},
        Stall = true
      }
    },
    Mattias = {
      {
        tAnims = {
          Player1 = "ALL02_Contract_Briefing_Mattias",
          Starter = "ALL02_Contract_Briefing_Starter-Mattias"
        },
        OnTime = 97.8,
        OnComplete = "Player1"
      },
      {
        tFlash = {
          sFile = "AllCon002_briefing.gfx",
          nTime = 34.4333
        },
        OnTime = 64.899994
      },
      {
        tCamera = {bHold = true},
        Stall = true
      }
    },
    tCameraEffects = {
      Chris = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2817,
            11.1642,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 0.77,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2817,
            9.3109,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 1.67,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2817,
            6.4803,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 5.43,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2817,
            1.7345,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 9.53,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2817,
            4.3536,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 12.8,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2817,
            3.0898,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 24,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2817,
            5.7351,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 30.33,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2817,
            3.505,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 36.93,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2817,
            4.7305,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 42.13,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2817,
            3.7962,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 45.47,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2817,
            1.5383,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 49.1,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2817,
            2.7311,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 54.67,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2817,
            3.7488,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 61.4,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2817,
            1.8396,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 62.63,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2817,
            1.717,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 64.63,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2817,
            2.253,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 71.37,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2817,
            4.0246,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 75.9,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2817,
            1.5872,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 89.47,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2817,
            2.2376,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 130.63,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2817,
            2.9113,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 134.83,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2817,
            1.9103,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 136.4,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2817,
            1.7723,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 144.07,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2817,
            1.7802,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 145.97,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2817,
            2.8631,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Jennifer = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.4428,
            11.3997,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 0.7,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.4428,
            9.2588,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 1.87,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.4428,
            2.2895,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 5.7,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.4428,
            1.913,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 10.2,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.4428,
            3.612,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 13.4,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.4428,
            3.1321,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 15.37,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.4428,
            4.6756,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 33.43,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.4428,
            3.1321,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 38.93,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.4428,
            4.1621,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 44.73,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.4428,
            3.1321,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 48.37,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.4428,
            1.4127,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 52.47,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.4428,
            3.1321,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 55.3,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.4428,
            3.1321,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 60.6,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.4428,
            1.8322,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 61.97,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.4428,
            1.918,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 63.77,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.4428,
            1.9639,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 70.5,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.4428,
            3.7998,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 72.3,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.4428,
            1.6622,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 86.33,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.4428,
            1.8175,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 126.67,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.4428,
            3.1321,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 131.3,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.4428,
            1.5411,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 132.8,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.4428,
            2.3627,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 140.17,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.4428,
            1.6789,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 141.8,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.4428,
            3.1321,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Mattias = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.4677,
            11.5892,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 1.03,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.4677,
            9.3509,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 2.17,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.4677,
            6.6057,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 6.9,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.4677,
            2.1286,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 11.53,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.4677,
            3.9028,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 14.67,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.4677,
            2.8372,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 18.73,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.4677,
            4.7864,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 36.3,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.4677,
            3.4932,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 42.37,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.4677,
            4.3697,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 48.07,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.4677,
            3.2329,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 51.43,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.4677,
            2.1286,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 55.83,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.4677,
            2.7426,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 63.87,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.4677,
            2.1286,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 65.23,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.4677,
            1.5629,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 67.83,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.4677,
            2.1286,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 74.4,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.4677,
            3.7947,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 81.73,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.4677,
            2.1286,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 95.27,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.4677,
            2.1286,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 139.67,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.4677,
            2.8392,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 144.33,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.4677,
            2.1286,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 145.97,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.4677,
            2.1286,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 153.53,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.4677,
            2.1286,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 156.37,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.4677,
            2.6222,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      }
    }
  },
  tDeclineCinematic = {
    Chris = {
      {
        tAnims = {
          Player1 = "ALL02_Contract_Briefing_No_Chris",
          Starter = "ALL02_Contract_Briefing_No_Starter-Chris"
        },
        OnComplete = "Player1"
      }
    },
    Jennifer = {
      {
        tAnims = {
          Player1 = "ALL02_Contract_Briefing_No_Jennifer",
          Starter = "ALL02_Contract_Briefing_No_Starter-Jennifer"
        },
        OnComplete = "Player1"
      }
    },
    Mattias = {
      {
        tAnims = {
          Player1 = "ALL02_Contract_Briefing_No_Mattias",
          Starter = "ALL02_Contract_Briefing_No_Starter-Mattias"
        },
        OnComplete = "Player1"
      }
    },
    tCameraEffects = {
      Chris = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2817,
            2.5929,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Jennifer = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.4428,
            3.1321,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Mattias = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.4677,
            2.1286,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      }
    }
  },
  tConfirmCinematic = {
    Chris = {
      {
        tAnims = {
          Player1 = "ALL02_Contract_Briefing_Yes_Chris",
          Starter = "ALL02_Contract_Briefing_Yes_Starter-Chris"
        },
        OnComplete = "Player1"
      }
    },
    Jennifer = {
      {
        tAnims = {
          Player1 = "ALL02_Contract_Briefing_Yes_Jennifer",
          Starter = "ALL02_Contract_Briefing_Yes_Starter-Jennifer"
        },
        OnComplete = "Player1"
      }
    },
    Mattias = {
      {
        tAnims = {
          Player1 = "ALL02_Contract_Briefing_Yes_Mattias",
          Starter = "ALL02_Contract_Briefing_Yes_Starter-Mattias"
        },
        OnComplete = "Player1"
      }
    },
    tCameraEffects = {
      Chris = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2817,
            2.5929,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Jennifer = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.4428,
            3.1321,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Mattias = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.4677,
            2.1286,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      }
    }
  }
}
AllCon003 = {
  tAssetPreload = {
    soundbank = {
      "vo_allCon004"
    },
    wavebank = {
      "vo_allCon004"
    }
  },
  tActors = {
    Starter = {
      sTemplate = "Allied Boss (Wheelchair)",
      sPosition = "Hp_starter"
    }
  },
  tPositions = {
    Player1 = "hp_al04_player",
    Starter = "hp_al04_starter"
  },
  tFaceAnimSets = {
    Player1 = {
      Chris = "ALL04_Contract_Briefing_Chris",
      Jennifer = "ALL04_Contract_Briefing_Jennifer",
      Mattias = "ALL04_Contract_Briefing_Mattias"
    },
    Starter = "ALL04_Contract_Briefing_Starter"
  },
  tCinematic = {
    Chris = {
      {
        tAnims = {
          Player1 = "ALL04_Contract_Briefing_Chris",
          Starter = "ALL04_Contract_Briefing_Starter-Chris"
        },
        OnTime = 43.3333,
        OnComplete = "Player1"
      },
      {
        tFlash = {
          sFile = "AllCon004_briefing.gfx",
          nTime = 26.8667
        },
        OnTime = 39.6667
      },
      {
        tCamera = {bHold = true},
        Stall = true
      }
    },
    Jennifer = {
      {
        tAnims = {
          Player1 = "ALL04_Contract_Briefing_Jennifer",
          Starter = "ALL04_Contract_Briefing_Starter-Jennifer"
        },
        OnTime = 40.6667,
        OnComplete = "Player1"
      },
      {
        tFlash = {
          sFile = "AllCon004_briefing.gfx",
          nTime = 25.6667
        },
        OnTime = 38.3333
      },
      {
        tCamera = {bHold = true},
        Stall = true
      }
    },
    Mattias = {
      {
        tAnims = {
          Player1 = "ALL04_Contract_Briefing_Mattias",
          Starter = "ALL04_Contract_Briefing_Starter-Mattias"
        },
        OnTime = 45.7,
        OnComplete = "Player1"
      },
      {
        tFlash = {
          sFile = "AllCon004_briefing.gfx",
          nTime = 25.9333
        },
        OnTime = 38.4667
      },
      {
        tCamera = {bHold = true},
        Stall = true
      }
    },
    tCameraEffects = {
      Chris = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2227,
            3.8348,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 5.97,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2227,
            3.9699,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 12.17,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2227,
            1.7314,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 15.07,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2227,
            4.4825,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 20.97,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2227,
            3.0149,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 26.2,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2227,
            1.5682,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 28.43,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2227,
            1.9741,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 35.93,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2227,
            3.2498,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 40.33,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2227,
            1.7475,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 72.97,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2227,
            1.9,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Jennifer = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2227,
            4.4825,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 3.63,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2227,
            4.4825,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 10.5,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2227,
            1.8202,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 13.23,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2227,
            4.4825,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 18.5,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2227,
            2.8808,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 24.03,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2227,
            1.635,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 26.13,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2227,
            2.032,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 33.37,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2227,
            3.1014,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 38.37,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2227,
            2.1111,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 69.7,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2227,
            2.0328,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Mattias = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2227,
            3.1211,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 4.2,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2227,
            3.6521,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 10.5,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2227,
            1.8063,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 13.37,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2227,
            3.5997,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 20.8,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2227,
            2.9807,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 26.2,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2227,
            1.7777,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 29.53,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2227,
            2.1352,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 36.7,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2227,
            2.9177,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 42.23,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2227,
            1.4713,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 74.67,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2227,
            1.6355,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      }
    }
  },
  tDeclineCinematic = {
    Chris = {
      {
        tAnims = {
          Player1 = "ALL04_Contract_Briefing_No_Chris",
          Starter = "ALL04_Contract_Briefing_No_Starter-Chris"
        },
        OnComplete = "Player1"
      }
    },
    Jennifer = {
      {
        tAnims = {
          Player1 = "ALL04_Contract_Briefing_No_Jennifer",
          Starter = "ALL04_Contract_Briefing_No_Starter-Jennifer"
        },
        OnComplete = "Player1"
      }
    },
    Mattias = {
      {
        tAnims = {
          Player1 = "ALL04_Contract_Briefing_No_Mattias",
          Starter = "ALL04_Contract_Briefing_No_Starter-Mattias"
        },
        OnComplete = "Player1"
      }
    },
    tCameraEffects = {
      Chris = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2227,
            1.8227,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Jennifer = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2227,
            2.521,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Mattias = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2227,
            1.6982,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      }
    }
  },
  tConfirmCinematic = {
    Chris = {
      {
        tAnims = {
          Player1 = "ALL04_Contract_Briefing_Yes_Chris",
          Starter = "ALL04_Contract_Briefing_Yes_Starter-Chris"
        },
        OnComplete = "Player1"
      }
    },
    Jennifer = {
      {
        tAnims = {
          Player1 = "ALL04_Contract_Briefing_Yes_Jennifer",
          Starter = "ALL04_Contract_Briefing_Yes_Starter-Jennifer"
        },
        OnComplete = "Player1"
      }
    },
    Mattias = {
      {
        tAnims = {
          Player1 = "ALL04_Contract_Briefing_Yes_Mattias",
          Starter = "ALL04_Contract_Briefing_Yes_Starter-Mattias"
        },
        OnComplete = "Player1"
      }
    },
    tCameraEffects = {
      Chris = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2227,
            1.8227,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Jennifer = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2227,
            2.521,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Mattias = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2227,
            1.6982,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      }
    }
  }
}
ChiCon001 = {
  nType = knSimple,
  tAssetPreload = {
    soundbank = {
      "vo_chinCon001"
    },
    wavebank = {
      "vo_chinCon001"
    }
  },
  tPositions = {
    Player1 = "hp_ch01_player",
    Starter = "hp_ch01_starter"
  },
  tFaceAnimSets = {
    Player1 = {
      Chris = "CHI01_Contract_Briefing_Chris",
      Jennifer = "CHI01_Contract_Briefing_Jennifer",
      Mattias = "CHI01_Contract_Briefing_Mattias"
    },
    Starter = "CHI01_Contract_Briefing_Starter"
  },
  tCinematic = {
    Chris = {
      {
        tAnims = {
          Player1 = "CHI01_Contract_Briefing_Chris",
          Starter = "CHI01_Contract_Briefing_Starter-Chris"
        },
        OnTime = 115.2,
        OnComplete = "Player1"
      },
      {
        tFlash = {
          sFile = "ChiCon001_briefing.gfx",
          nTime = 36.8
        },
        OnTime = 47.130005
      },
      {
        tCamera = {bHold = true},
        Stall = true
      }
    },
    Jennifer = {
      {
        tAnims = {
          Player1 = "CHI01_Contract_Briefing_Jennifer",
          Starter = "CHI01_Contract_Briefing_Starter-Jennifer"
        },
        OnTime = 116.2333,
        OnComplete = "Player1"
      },
      {
        tFlash = {
          sFile = "ChiCon001_briefing.gfx",
          nTime = 35.5
        },
        OnTime = 46.166695
      },
      {
        tCamera = {bHold = true},
        Stall = true
      }
    },
    Mattias = {
      {
        tAnims = {
          Player1 = "CHI01_Contract_Briefing_Mattias",
          Starter = "CHI01_Contract_Briefing_Starter-Mattias"
        },
        OnTime = 124.03,
        OnComplete = "Player1"
      },
      {
        tFlash = {
          sFile = "ChiCon001_briefing.gfx",
          nTime = 37.1
        },
        OnTime = 47.136703
      },
      {
        tCamera = {bHold = true},
        Stall = true
      }
    },
    tCameraEffects = {
      Chris = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.3118,
            8.3282,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 1.7,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.3118,
            8.333,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 4.13,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.3118,
            2.0478,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 13.63,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.3118,
            2.0835,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 17.4,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.3118,
            2.083,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 28.6,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.3118,
            3.0927,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 36.8,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.3118,
            3.0927,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 47.8,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.3118,
            1.9254,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 56.33,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.3118,
            1.9295,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 69.13,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.3118,
            1.6249,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 75.23,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.3118,
            1.6331,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 77.67,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.3118,
            1.363,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 80.87,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.3118,
            1.363,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 82.4,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.3118,
            1.3209,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 85.67,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.3118,
            1.3213,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 94.53,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.3118,
            2.4651,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 101.17,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.3118,
            2.6028,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 109.63,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.3118,
            6.7271,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 159.63,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.3118,
            4.2542,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Jennifer = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.483,
            2.639,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 1.67,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            4.745,
            7.7865,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 5.8,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.6472,
            2.378,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 13.83,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2011,
            2.7799,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 17.17,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.3536,
            2.3046,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 29,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2597,
            3.0927,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 36.53,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2979,
            3.0927,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 47.5,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2144,
            1.9254,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 56.3,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2198,
            3.4069,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 71.43,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2291,
            1.4113,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 75.07,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.3275,
            1.9728,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 77.57,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2471,
            2.3794,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 80.57,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.159,
            1.6634,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 82.8,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1527,
            1.6807,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 85.9,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.466,
            3,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 94.5,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.476,
            1.3638,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 101.53,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.9757,
            2.6028,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 110.17,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2643,
            2.2268,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 159.43,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.6609,
            1.7321,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Mattias = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.6477,
            6.7271,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 1.37,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            4.1119,
            8.333,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 4,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.0896,
            2.778,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 12.5,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.0896,
            3.472,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 16.7,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.0896,
            2.083,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 28.77,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.0896,
            3.0927,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 36.23,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.0896,
            3.0927,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 47.2,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.0896,
            1.9254,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 58,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.0896,
            1.9254,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 71.13,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.0896,
            1.4113,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 77.47,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.0896,
            1.16,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 80.13,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.0896,
            1.16,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 83.8,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.0896,
            1.16,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 88.2,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.0896,
            1.3209,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 91.27,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.0896,
            2.435,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 96.53,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.0896,
            1.9199,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 106.9,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.0896,
            2.6028,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 117.83,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.0896,
            1.742,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 168.43,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.0896,
            1.5783,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      }
    }
  },
  tDeclineCinematic = {
    Chris = {
      {
        tAnims = {
          Player1 = "CHI01_Contract_Briefing_No_Chris",
          Starter = "CHI01_Contract_Briefing_No_Starter-Chris"
        },
        OnComplete = "Player1"
      }
    },
    Jennifer = {
      {
        tAnims = {
          Player1 = "CHI01_Contract_Briefing_No_Jennifer",
          Starter = "CHI01_Contract_Briefing_No_Starter-Jennifer"
        },
        OnComplete = "Player1"
      }
    },
    Mattias = {
      {
        tAnims = {
          Player1 = "CHI01_Contract_Briefing_No_Mattias",
          Starter = "CHI01_Contract_Briefing_No_Starter-Mattias"
        },
        OnComplete = "Player1"
      }
    },
    tCameraEffects = {
      Chris = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.3118,
            3.0367,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Jennifer = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.483,
            2.639,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Mattias = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.0896,
            2.2273,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      }
    }
  },
  tConfirmCinematic = {
    Chris = {
      {
        tAnims = {
          Player1 = "CHI01_Contract_Briefing_Yes_Chris",
          Starter = "CHI01_Contract_Briefing_Yes_Starter-Chris"
        },
        OnComplete = "Player1"
      }
    },
    Jennifer = {
      {
        tAnims = {
          Player1 = "CHI01_Contract_Briefing_Yes_Jennifer",
          Starter = "CHI01_Contract_Briefing_Yes_Starter-Jennifer"
        },
        OnComplete = "Player1"
      }
    },
    Mattias = {
      {
        tAnims = {
          Player1 = "CHI01_Contract_Briefing_Yes_Mattias",
          Starter = "CHI01_Contract_Briefing_Yes_Starter-Mattias"
        },
        OnComplete = "Player1"
      }
    },
    tCameraEffects = {
      Chris = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.3118,
            3.0367,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Jennifer = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.483,
            2.639,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Mattias = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.0896,
            2.2273,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      }
    }
  }
}
ChiCon002 = {
  tAssetPreload = {
    soundbank = {
      "vo_chinCon002"
    },
    wavebank = {
      "vo_chinCon002"
    }
  },
  tPositions = {
    Player1 = "hp_ch02_player",
    Starter = "hp_ch02_starter"
  },
  tFaceAnimSets = {
    Player1 = {
      Chris = "CHI02_Contract_Briefing_Chris",
      Jennifer = "CHI02_Contract_Briefing_Jennifer",
      Mattias = "CHI02_Contract_Briefing_Mattias"
    },
    Starter = "CHI02_Contract_Briefing_Starter"
  },
  tCinematic = {
    Chris = {
      {
        tAnims = {
          Player1 = "CHI02_Contract_Briefing_Chris",
          Starter = "CHI02_Contract_Briefing_Starter-Chris"
        },
        OnTime = 28.1,
        OnComplete = "Player1"
      },
      {
        tFlash = {
          sFile = "ChiCon002_briefing.gfx",
          nTime = 31.9333
        },
        OnTime = 47.5
      },
      {
        tCamera = {bHold = true},
        Stall = true
      }
    },
    Jennifer = {
      {
        tAnims = {
          Player1 = "CHI02_Contract_Briefing_Jennifer",
          Starter = "CHI02_Contract_Briefing_Starter-Jennifer"
        },
        OnTime = 25,
        OnComplete = "Player1"
      },
      {
        tFlash = {
          sFile = "ChiCon002_briefing.gfx",
          nTime = 32.0667
        },
        OnTime = 46.5
      },
      {
        tCamera = {bHold = true},
        Stall = true
      }
    },
    Mattias = {
      {
        tAnims = {
          Player1 = "CHI02_Contract_Briefing_Mattias",
          Starter = "CHI02_Contract_Briefing_Starter-Mattias"
        },
        OnTime = 24.9667,
        OnComplete = "Player1"
      },
      {
        tFlash = {
          sFile = "ChiCon002_briefing.gfx",
          nTime = 29.8333
        },
        OnTime = 46.533302
      },
      {
        tCamera = {bHold = true},
        Stall = true
      }
    },
    tCameraEffects = {
      Chris = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.8858,
            6.9402,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            46.3078,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 1.5,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.6048,
            2.6991,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            46.3078,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 5.43,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.581,
            2.4952,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            46.3078,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 13,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1329,
            4.07,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            46.3078,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 24.37,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.3142,
            1.9924,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            46.3078,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 26.9,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2262,
            1.9924,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            46.3078,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 58.63,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2262,
            1.9924,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            46.3078,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 62.73,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2262,
            1.9924,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            46.3078,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 67.57,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.357,
            2.184,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            46.3078,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 70.9,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.3507,
            2.184,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            46.3078,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Jennifer = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.8858,
            6.9402,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 1.7,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.6048,
            2.6991,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 4.53,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.581,
            2.4952,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 10.77,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.581,
            2.4952,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 21.53,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.3142,
            1.9924,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 24.13,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2262,
            1.9924,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 57.07,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2262,
            1.9924,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 58.8,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2262,
            1.9924,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 64.07,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.357,
            2.184,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 66.37,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.3507,
            2.184,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Mattias = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.8858,
            6.9402,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 1.2,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.6048,
            2.6991,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 5.03,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.581,
            2.4952,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 10.83,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1329,
            4.07,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 21.87,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.3142,
            1.9924,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 23.87,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2262,
            1.9924,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 54.8,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2262,
            1.9924,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 57.83,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2262,
            1.9924,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 63.03,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.357,
            2.184,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 66.4,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.3507,
            2.184,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      }
    }
  },
  tDeclineCinematic = {
    Chris = {
      {
        tAnims = {
          Player1 = "CHI02_Contract_Briefing_No_Chris",
          Starter = "CHI02_Contract_Briefing_No_Starter-Chris"
        },
        OnComplete = "Player1"
      }
    },
    Jennifer = {
      {
        tAnims = {
          Player1 = "CHI02_Contract_Briefing_No_Jennifer",
          Starter = "CHI02_Contract_Briefing_No_Starter-Jennifer"
        },
        OnComplete = "Player1"
      }
    },
    Mattias = {
      {
        tAnims = {
          Player1 = "CHI02_Contract_Briefing_No_Mattias",
          Starter = "CHI02_Contract_Briefing_No_Starter-Mattias"
        },
        OnComplete = "Player1"
      }
    },
    tCameraEffects = {
      Chris = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.8858,
            6.9402,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            46.3078,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Jennifer = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.8858,
            6.9402,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Mattias = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2262,
            1.9924,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      }
    }
  },
  tConfirmCinematic = {
    Chris = {
      {
        tAnims = {
          Player1 = "CHI02_Contract_Briefing_Yes_Chris",
          Starter = "CHI02_Contract_Briefing_Yes_Starter-Chris"
        },
        OnComplete = "Player1"
      }
    },
    Jennifer = {
      {
        tAnims = {
          Player1 = "CHI02_Contract_Briefing_Yes_Jennifer",
          Starter = "CHI02_Contract_Briefing_Yes_Starter-Jennifer"
        },
        OnComplete = "Player1"
      }
    },
    Mattias = {
      {
        tAnims = {
          Player1 = "CHI02_Contract_Briefing_Yes_Mattias",
          Starter = "CHI02_Contract_Briefing_Yes_Starter-Mattias"
        },
        OnComplete = "Player1"
      }
    },
    tCameraEffects = {
      Chris = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.8858,
            6.9402,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            46.3078,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Jennifer = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.8858,
            6.9402,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Mattias = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2262,
            1.9924,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      }
    }
  }
}
ChiCon003 = {
  tAssetPreload = {
    soundbank = {
      "vo_chinCon004"
    },
    wavebank = {
      "vo_chinCon004"
    }
  },
  tPositions = {
    Player1 = "hp_ch04_player",
    Starter = "hp_ch04_starter"
  },
  tFaceAnimSets = {
    Player1 = {
      Chris = "CHI04_Contract_Briefing_Chris",
      Jennifer = "CHI04_Contract_Briefing_Jennifer",
      Mattias = "CHI04_Contract_Briefing_Mattias"
    },
    Starter = "CHI04_Contract_Briefing_Starter"
  },
  tCinematic = {
    Chris = {
      {
        tAnims = {
          Player1 = "CHI04_Contract_Briefing_Chris",
          Starter = "CHI04_Contract_Briefing_Starter-Chris"
        },
        OnTime = 23.6,
        OnComplete = "Player1"
      },
      {
        tFlash = {
          sFile = "ChiCon004_briefing.gfx",
          nTime = 22.6667
        },
        OnTime = 30.566698
      },
      {
        tCamera = {bHold = true},
        Stall = true
      }
    },
    Jennifer = {
      {
        tAnims = {
          Player1 = "CHI04_Contract_Briefing_Jennifer",
          Starter = "CHI04_Contract_Briefing_Starter-Jennifer"
        },
        OnTime = 21.3333,
        OnComplete = "Player1"
      },
      {
        tFlash = {
          sFile = "ChiCon004_briefing.gfx",
          nTime = 22.6
        },
        OnTime = 30.766699
      },
      {
        tCamera = {bHold = true},
        Stall = true
      }
    },
    Mattias = {
      {
        tAnims = {
          Player1 = "CHI04_Contract_Briefing_Mattias",
          Starter = "CHI04_Contract_Briefing_Starter-Mattias"
        },
        OnTime = 22.7,
        OnComplete = "Player1"
      },
      {
        tFlash = {
          sFile = "ChiCon004_briefing.gfx",
          nTime = 24.6667
        },
        OnTime = 39.333298
      },
      {
        tCamera = {bHold = true},
        Stall = true
      }
    },
    tCameraEffects = {
      Chris = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.4903,
            3.8005,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            81,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 8.57,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.478,
            2.4809,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            81,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 17.17,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.5015,
            2.5945,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            60,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 48.9,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2295,
            2.1388,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            60,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Jennifer = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.7902,
            5.7986,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            60,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 7.37,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.7785,
            3.4809,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            70,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 15,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.4011,
            3.5944,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            60,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 46.83,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1293,
            3.1388,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            60,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Mattias = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.7903,
            5.8005,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            60,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 7.53,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.7663,
            3.4809,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            70,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 16.2,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.65,
            3.4894,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            60,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 47.37,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.7285,
            3.4894,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            60,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 57.47,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.6402,
            3.4894,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            60,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      }
    }
  },
  tDeclineCinematic = {
    Chris = {
      {
        tAnims = {
          Player1 = "CHI04_Contract_Briefing_No_Chris",
          Starter = "CHI04_Contract_Briefing_No_Starter-Chris"
        },
        OnComplete = "Player1"
      }
    },
    Jennifer = {
      {
        tAnims = {
          Player1 = "CHI04_Contract_Briefing_No_Jennifer",
          Starter = "CHI04_Contract_Briefing_No_Starter-Jennifer"
        },
        OnComplete = "Player1"
      }
    },
    Mattias = {
      {
        tAnims = {
          Player1 = "CHI04_Contract_Briefing_No_Mattias",
          Starter = "CHI04_Contract_Briefing_No_Starter-Mattias"
        },
        OnComplete = "Player1"
      }
    },
    tCameraEffects = {
      Chris = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2348,
            1.9113,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            60,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Jennifer = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.3169,
            2.1388,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            60,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Mattias = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.6402,
            2.0519,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            60,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      }
    }
  },
  tConfirmCinematic = {
    Chris = {
      {
        tAnims = {
          Player1 = "CHI04_Contract_Briefing_Yes_Chris",
          Starter = "CHI04_Contract_Briefing_Yes_Starter-Chris"
        },
        OnComplete = "Player1"
      }
    },
    Jennifer = {
      {
        tAnims = {
          Player1 = "CHI04_Contract_Briefing_Yes_Jennifer",
          Starter = "CHI04_Contract_Briefing_Yes_Starter-Jennifer"
        },
        OnComplete = "Player1"
      }
    },
    Mattias = {
      {
        tAnims = {
          Player1 = "CHI04_Contract_Briefing_Yes_Mattias",
          Starter = "CHI04_Contract_Briefing_Yes_Starter-Mattias"
        },
        OnComplete = "Player1"
      }
    },
    tCameraEffects = {
      Chris = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2348,
            1.9113,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            60,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Jennifer = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.3169,
            2.1388,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            60,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Mattias = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.6402,
            2.0519,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            60,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      }
    }
  }
}
GurCon001 = {
  nType = knSimple,
  tAssetPreload = {
    soundbank = {
      "vo_gurCon001"
    },
    wavebank = {
      "vo_gurCon001"
    }
  },
  tPositions = {
    Player1 = "hp_gr001_player",
    Starter = "hp_gr001_starter"
  },
  tFaceAnimSets = {
    Player1 = {
      Chris = "GUR001_Contract_Briefing_Chris",
      Jennifer = "GUR001_Contract_Briefing_Jennifer",
      Mattias = "GUR001_Contract_Briefing_Mattias"
    },
    Starter = "GUR001_Contract_Briefing_Starter"
  },
  tCinematic = {
    Chris = {
      {
        tAnims = {
          Player1 = "GUR01_Contract_Briefing_Chris",
          Starter = "GUR01_Contract_Briefing_Starter-Chris"
        },
        OnTime = 26.9667,
        OnComplete = "Player1"
      },
      {
        tFlash = {
          sFile = "GurCon001_briefing.gfx",
          nTime = 16.7333
        },
        OnTime = 23.000002
      },
      {
        tCamera = {bHold = true},
        Stall = true
      }
    },
    Jennifer = {
      {
        tAnims = {
          Player1 = "GUR01_Contract_Briefing_Jennifer",
          Starter = "GUR01_Contract_Briefing_Starter-Jennifer"
        },
        OnTime = 26.8333,
        OnComplete = "Player1"
      },
      {
        tFlash = {
          sFile = "GurCon001_briefing.gfx",
          nTime = 16
        },
        OnTime = 22.7334
      },
      {
        tCamera = {bHold = true},
        Stall = true
      }
    },
    Mattias = {
      {
        tAnims = {
          Player1 = "GUR01_Contract_Briefing_Mattias",
          Starter = "GUR01_Contract_Briefing_Starter-Mattias"
        },
        OnTime = 25.3667,
        OnComplete = "Player1"
      },
      {
        tFlash = {
          sFile = "GurCon001_briefing.gfx",
          nTime = 20.5667
        },
        OnTime = 27.6666
      },
      {
        tCamera = {bHold = true},
        Stall = true
      }
    },
    tCameraEffects = {
      Chris = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1809,
            5.337,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 1.4,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1809,
            5.5783,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 5.93,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1809,
            3.4859,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 13.8,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1809,
            4.1465,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 22.23,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1809,
            2.5008,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Jennifer = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1809,
            4.1465,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 1.4,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1809,
            4.1465,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 5.93,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1809,
            3.1201,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 14.4,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1809,
            4.1465,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 22.37,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1809,
            2.4582,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Mattias = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1809,
            4.1465,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 1.4,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1809,
            4.1465,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 5.93,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1809,
            4.1465,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 12.73,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1809,
            4.1465,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 19.87,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1809,
            4.1465,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      }
    }
  },
  tDeclineCinematic = {
    Chris = {
      {
        tAnims = {
          Player1 = "GUR01_Contract_Briefing_No_Chris",
          Starter = "GUR01_Contract_Briefing_No_Starter-Chris"
        },
        OnComplete = "Player1"
      }
    },
    Jennifer = {
      {
        tAnims = {
          Player1 = "GUR01_Contract_Briefing_No_Jennifer",
          Starter = "GUR01_Contract_Briefing_No_Starter-Jennifer"
        },
        OnComplete = "Player1"
      }
    },
    Mattias = {
      {
        tAnims = {
          Player1 = "GUR01_Contract_Briefing_No_Mattias",
          Starter = "GUR01_Contract_Briefing_No_Starter-Mattias"
        },
        OnComplete = "Player1"
      }
    },
    tCameraEffects = {
      Chris = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1809,
            2.3776,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Jennifer = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1809,
            2.2028,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Mattias = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1809,
            4.1465,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      }
    }
  },
  tConfirmCinematic = {
    Chris = {
      {
        tAnims = {
          Player1 = "GUR01_Contract_Briefing_Yes_Chris",
          Starter = "GUR01_Contract_Briefing_Yes_Starter-Chris"
        },
        OnComplete = "Player1"
      }
    },
    Jennifer = {
      {
        tAnims = {
          Player1 = "GUR01_Contract_Briefing_Yes_Jennifer",
          Starter = "GUR01_Contract_Briefing_Yes_Starter-Jennifer"
        },
        OnComplete = "Player1"
      }
    },
    Mattias = {
      {
        tAnims = {
          Player1 = "GUR01_Contract_Briefing_Yes_Mattias",
          Starter = "GUR01_Contract_Briefing_Yes_Starter-Mattias"
        },
        OnComplete = "Player1"
      }
    },
    tCameraEffects = {
      Chris = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1809,
            2.3776,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Jennifer = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1809,
            2.2028,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Mattias = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1809,
            4.1465,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      }
    }
  }
}
GurCon002 = {
  tAssetPreload = {
    soundbank = {
      "vo_gurCon002"
    },
    wavebank = {
      "vo_gurCon002"
    }
  },
  tPositions = {
    Player1 = "hp_gr002_player",
    Starter = "hp_gr002_starter"
  },
  tFaceAnimSets = {
    Player1 = {
      Chris = "GUR002_Contract_Briefing_Chris",
      Jennifer = "GUR002_Contract_Briefing_Jennifer",
      Mattias = "GUR002_Contract_Briefing_Mattias"
    },
    Starter = "GUR002_Contract_Briefing_Starter"
  },
  tCinematic = {
    Chris = {
      {
        tAnims = {
          Player1 = "GUR02_Contract_Briefing_Chris",
          Starter = "GUR02_Contract_Briefing_Starter-Chris"
        },
        OnTime = 88.8333,
        OnComplete = "Player1"
      },
      {
        tFlash = {
          sFile = "GurCon002_briefing.gfx",
          nTime = 35.8667
        },
        OnTime = 37.5334
      },
      {
        tCamera = {bHold = true},
        Stall = true
      }
    },
    Jennifer = {
      {
        tAnims = {
          Player1 = "GUR02_Contract_Briefing_Jennifer",
          Starter = "GUR02_Contract_Briefing_Starter-Jennifer"
        },
        OnTime = 74.3,
        OnComplete = "Player1"
      },
      {
        tFlash = {
          sFile = "GurCon002_briefing.gfx",
          nTime = 35.9
        },
        OnTime = 37.833298
      },
      {
        tCamera = {bHold = true},
        Stall = true
      }
    },
    Mattias = {
      {
        tAnims = {
          Player1 = "GUR02_Contract_Briefing_Mattias",
          Starter = "GUR02_Contract_Briefing_Starter-Mattias"
        },
        OnTime = 87,
        OnComplete = "Player1"
      },
      {
        tFlash = {
          sFile = "GurCon002_briefing.gfx",
          nTime = 36
        },
        OnTime = 38.7333
      },
      {
        tCamera = {bHold = true},
        Stall = true
      }
    },
    tCameraEffects = {
      Chris = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.3154,
            2.1758,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 2.73,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.3154,
            5.5056,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 7.23,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.3154,
            3.2026,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 15.3,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.3154,
            2.3048,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 21.73,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.3154,
            1.6314,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 34.47,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.3154,
            2.3361,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 37.2,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.3154,
            3.6805,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 41.93,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.3154,
            2.1553,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 56.67,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.3154,
            3.2026,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 59.43,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.3154,
            1.7525,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 70.9,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.3154,
            3.2026,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 75.47,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.3154,
            1.8848,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 80.93,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.3154,
            3.2026,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 126.8,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.3154,
            1.7859,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Jennifer = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1676,
            2.2519,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 2.23,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1676,
            3.6269,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 4.43,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1676,
            3.2026,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 7.4,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1676,
            2.2417,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 8.7,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1676,
            2.1361,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 21.33,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1676,
            2.1777,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 25.33,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1676,
            3.2026,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 30.03,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1676,
            2.1971,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 44.83,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1676,
            3.2026,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 47.57,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1676,
            1.7487,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 59.13,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1676,
            3.2026,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 63.1,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1676,
            1.7342,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 68.73,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1676,
            3.2026,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 112.37,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1676,
            1.792,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Mattias = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.5504,
            2.2318,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 7.57,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.5504,
            3.2026,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 11.73,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.5504,
            2.4152,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 15.63,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.5504,
            1.6822,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 28.17,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.5504,
            1.8155,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 31.5,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.5504,
            3.8053,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 38.6,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.5504,
            2.4619,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 53.17,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.5504,
            3.2026,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 56.4,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.5504,
            2.162,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 68.5,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.5504,
            3.2026,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 73.9,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.5504,
            1.8911,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 79.4,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.5504,
            3.2026,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 126.4,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.5504,
            1.558,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      }
    }
  },
  tDeclineCinematic = {
    Chris = {
      {
        tAnims = {
          Player1 = "GUR02_Contract_Briefing_No_Chris",
          Starter = "GUR02_Contract_Briefing_No_Starter-Chris"
        },
        OnComplete = "Player1"
      }
    },
    Jennifer = {
      {
        tAnims = {
          Player1 = "GUR02_Contract_Briefing_No_Jennifer",
          Starter = "GUR02_Contract_Briefing_No_Starter-Jennifer"
        },
        OnComplete = "Player1"
      }
    },
    Mattias = {
      {
        tAnims = {
          Player1 = "GUR02_Contract_Briefing_No_Mattias",
          Starter = "GUR02_Contract_Briefing_No_Starter-Mattias"
        },
        OnComplete = "Player1"
      }
    },
    tCameraEffects = {
      Chris = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.3154,
            3.2026,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Jennifer = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1676,
            3.2026,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Mattias = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.5504,
            3.2026,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      }
    }
  },
  tConfirmCinematic = {
    Chris = {
      {
        tAnims = {
          Player1 = "GUR02_Contract_Briefing_Yes_Chris",
          Starter = "GUR02_Contract_Briefing_Yes_Starter-Chris"
        },
        OnComplete = "Player1"
      }
    },
    Jennifer = {
      {
        tAnims = {
          Player1 = "GUR02_Contract_Briefing_Yes_Jennifer",
          Starter = "GUR02_Contract_Briefing_Yes_Starter-Jennifer"
        },
        OnComplete = "Player1"
      }
    },
    Mattias = {
      {
        tAnims = {
          Player1 = "GUR02_Contract_Briefing_Yes_Mattias",
          Starter = "GUR02_Contract_Briefing_Yes_Starter-Mattias"
        },
        OnComplete = "Player1"
      }
    },
    tCameraEffects = {
      Chris = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.3154,
            3.2026,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Jennifer = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1676,
            3.2026,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Mattias = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.5504,
            3.2026,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      }
    }
  }
}
OilCon001 = {
  tAssetPreload = {
    soundbank = {
      "vo_oilCon001"
    },
    wavebank = {
      "vo_oilCon001"
    }
  },
  tPositions = {
    Player1 = "hp_oc01_player",
    Starter = "hp_oc01_starter"
  },
  tFaceAnimSets = {
    Player1 = {
      Chris = "OIL01_Contract_Briefing_Chris",
      Jennifer = "OIL01_Contract_Briefing_Jennifer",
      Mattias = "OIL01_Contract_Briefing_Mattias"
    },
    Starter = "OIL01_Contract_Briefing_Starter"
  },
  tCinematic = {
    Chris = {
      {
        tAnims = {
          Player1 = "OIL01_Contract_Briefing_Chris",
          Starter = "OIL01_Contract_Briefing_Starter-Chris"
        },
        OnTime = 28.1667,
        OnComplete = "Player1"
      },
      {
        tFlash = {
          sFile = "OilCon001_briefing.gfx",
          nTime = 25
        },
        OnTime = 32.5
      },
      {
        tCamera = {bHold = true},
        Stall = true
      }
    },
    Jennifer = {
      {
        tAnims = {
          Player1 = "OIL01_Contract_Briefing_Jennifer",
          Starter = "OIL01_Contract_Briefing_Starter-Jennifer"
        },
        OnTime = 30,
        OnComplete = "Player1"
      },
      {
        tFlash = {
          sFile = "OilCon001_briefing.gfx",
          nTime = 24.7333
        },
        OnTime = 33
      },
      {
        tCamera = {bHold = true},
        Stall = true
      }
    },
    Mattias = {
      {
        tAnims = {
          Player1 = "OIL01_Contract_Briefing_Mattias",
          Starter = "OIL01_Contract_Briefing_Starter-Mattias"
        },
        OnTime = 32.4667,
        OnComplete = "Player1"
      },
      {
        tFlash = {
          sFile = "OilCon001_briefing.gfx",
          nTime = 25.3667
        },
        OnTime = 33.4333
      },
      {
        tCamera = {bHold = true},
        Stall = true
      }
    },
    tCameraEffects = {
      Chris = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1877,
            5.5694,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            90,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 5.23,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1877,
            9.0594,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 7.5,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1877,
            3.2532,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 9.23,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1877,
            5.5694,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 13.13,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1877,
            2.271,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 16.57,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1877,
            3.8421,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 19.5,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1877,
            2.8975,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 26.2,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1877,
            2.0117,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 27.4,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1877,
            2.5992,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 56.87,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1877,
            3.7571,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Jennifer = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1877,
            5.5694,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            90,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 4.77,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1877,
            8.6709,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 7,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1877,
            2.3288,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 9.23,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1877,
            5.5694,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 13.5,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1877,
            2.4782,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 18.43,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1877,
            4.0744,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 21.53,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1877,
            3.5161,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 27.97,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1877,
            1.903,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 29.27,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1877,
            2.3083,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 59.13,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1877,
            4.2672,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Mattias = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.333,
            6.2618,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            90,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 4.73,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.333,
            9.5611,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 7,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.333,
            2.7151,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 9.23,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.333,
            6.9925,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 13.57,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.333,
            2.5525,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 17.33,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.333,
            4.4663,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 20.77,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.333,
            3.8858,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 28.93,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.333,
            2.0385,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 31.23,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.333,
            2.8017,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 61.7,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.333,
            4.2029,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      }
    }
  },
  tDeclineCinematic = {
    Chris = {
      {
        tAnims = {
          Player1 = "OIL01_Contract_Briefing_No_Chris",
          Starter = "OIL01_Contract_Briefing_No_Starter-Chris"
        },
        OnComplete = "Player1"
      }
    },
    Jennifer = {
      {
        tAnims = {
          Player1 = "OIL01_Contract_Briefing_No_Jennifer",
          Starter = "OIL01_Contract_Briefing_No_Starter-Jennifer"
        },
        OnComplete = "Player1"
      }
    },
    Mattias = {
      {
        tAnims = {
          Player1 = "OIL01_Contract_Briefing_No_Mattias",
          Starter = "OIL01_Contract_Briefing_No_Starter-Mattias"
        },
        OnComplete = "Player1"
      }
    },
    tCameraEffects = {
      Chris = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1877,
            3.1776,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Jennifer = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1877,
            3.1938,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Mattias = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.333,
            3.6993,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      }
    }
  },
  tConfirmCinematic = {
    Chris = {
      {
        tAnims = {
          Player1 = "OIL01_Contract_Briefing_Yes_Chris",
          Starter = "OIL01_Contract_Briefing_Yes_Starter-Chris"
        },
        OnComplete = "Player1"
      }
    },
    Jennifer = {
      {
        tAnims = {
          Player1 = "OIL01_Contract_Briefing_Yes_Jennifer",
          Starter = "OIL01_Contract_Briefing_Yes_Starter-Jennifer"
        },
        OnComplete = "Player1"
      }
    },
    Mattias = {
      {
        tAnims = {
          Player1 = "OIL01_Contract_Briefing_Yes_Mattias",
          Starter = "OIL01_Contract_Briefing_Yes_Starter-Mattias"
        },
        OnComplete = "Player1"
      }
    },
    tCameraEffects = {
      Chris = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1877,
            3.1776,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Jennifer = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1877,
            3.1938,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Mattias = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.333,
            3.6993,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      }
    }
  }
}
OilCon002 = {
  tAssetPreload = {
    soundbank = {
      "vo_oilCon002"
    },
    wavebank = {
      "vo_oilCon002"
    }
  },
  tPositions = {
    Player1 = "hp_oc02_player",
    Starter = "hp_oc02_starter"
  },
  tFaceAnimSets = {
    Player1 = {
      Chris = "OIL02_Contract_Briefing_Chris",
      Jennifer = "OIL02_Contract_Briefing_Jennifer",
      Mattias = "OIL02_Contract_Briefing_Mattias"
    },
    Starter = "OIL02_Contract_Briefing_Starter"
  },
  tCinematic = {
    Chris = {
      {
        tAnims = {
          Player1 = "OIL02_Contract_Briefing_Chris",
          Starter = "OIL02_Contract_Briefing_Starter-Chris"
        },
        OnTime = 83.0667,
        OnComplete = "Player1"
      },
      {
        tFlash = {
          sFile = "OilCon002_briefing.gfx",
          nTime = 33.7667
        },
        OnTime = 48.933296
      },
      {
        tCamera = {bHold = true},
        Stall = true
      }
    },
    Jennifer = {
      {
        tAnims = {
          Player1 = "OIL02_Contract_Briefing_Jennifer",
          Starter = "OIL02_Contract_Briefing_Starter-Jennifer"
        },
        OnTime = 84.9,
        OnComplete = "Player1"
      },
      {
        tFlash = {
          sFile = "OilCon002_briefing.gfx",
          nTime = 33.2667
        },
        OnTime = 48.6
      },
      {
        tCamera = {bHold = true},
        Stall = true
      }
    },
    Mattias = {
      {
        tAnims = {
          Player1 = "OIL02_Contract_Briefing_Mattias",
          Starter = "OIL02_Contract_Briefing_Starter-Mattias"
        },
        OnTime = 82.0333,
        OnComplete = "Player1"
      },
      {
        tFlash = {
          sFile = "OilCon002_briefing.gfx",
          nTime = 33.5
        },
        OnTime = 49.6334
      },
      {
        tCamera = {bHold = true},
        Stall = true
      }
    },
    tCameraEffects = {
      Chris = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2763,
            5.0596,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 8.33,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2763,
            3.314,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 15.3,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2763,
            1.9329,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 19,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2763,
            3.7045,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 22.63,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2763,
            2.3317,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 25.7,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2763,
            2.2749,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 31.23,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2763,
            4.1047,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 34.97,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2763,
            3.0578,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 38.03,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2763,
            3.2066,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 43.33,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2763,
            3.1239,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 45.7,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2763,
            3.1239,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 51.5,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2763,
            2.7886,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 53.57,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2763,
            2.2151,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 56.43,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2763,
            2.1233,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 58.77,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2763,
            5.0596,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 68.83,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2763,
            1.5619,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 69.87,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2763,
            2.93,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 72.13,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2763,
            5.0596,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 75,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2763,
            2.6005,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 81.03,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2763,
            2.7251,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 119.1,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2763,
            3.4792,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 124.93,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2763,
            1.6637,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 126.63,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2763,
            2.6394,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Jennifer = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2086,
            5.0017,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 8.33,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2086,
            2.9319,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 21.07,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2086,
            2.3648,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 24.73,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2086,
            2.1252,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 27.37,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2086,
            2.2257,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 32.9,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2086,
            4.3699,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 36.53,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2086,
            3.4685,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 39.53,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2086,
            4.0085,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 43.17,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2086,
            3.4449,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 46.7,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2086,
            3.4449,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 52.43,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2086,
            2.0181,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 54.37,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2086,
            1.5743,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 58.37,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2086,
            2.1027,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 62.17,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2086,
            4.8849,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 70.57,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2086,
            1.5048,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 71.63,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2086,
            3.0621,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 73.7,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2086,
            3.7253,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 76.83,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2086,
            2.4788,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 82.93,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2086,
            2.0117,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 120.37,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2086,
            3.686,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 126.1,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2086,
            2.234,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 128.1,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2086,
            2.7388,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Mattias = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            1.8166,
            6.1371,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 8.33,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.3888,
            3.7639,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 15.23,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.3057,
            6.3614,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 18.93,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            1.0234,
            5.7332,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 22.63,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.5653,
            5.3393,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 25.3,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.6832,
            5.8549,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 30.7,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            1.0934,
            8.3319,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 35.37,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            1.3307,
            6.8709,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 37.4,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.4445,
            7.911,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 42.93,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.3172,
            8.4144,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 46.03,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.3172,
            2.3461,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 51.73,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.4938,
            9.1556,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 53.87,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            1.1491,
            8.2122,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 56.07,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.956,
            6.5908,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 58.77,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            1.1618,
            10.3124,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 67.57,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.8212,
            6.583,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 68.57,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            1.0236,
            6.5112,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 70.3,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            1.5977,
            9.0242,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 73.27,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.8881,
            6.4034,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 79.67,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.3992,
            7.0673,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 118.37,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.9127,
            7.5036,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 124.6,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.4621,
            3.5197,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 126.6,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.853,
            3.4331,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      }
    }
  },
  tDeclineCinematic = {
    Chris = {
      {
        tAnims = {
          Player1 = "OIL02_Contract_Briefing_No_Chris",
          Starter = "OIL02_Contract_Briefing_No_Starter-Chris"
        },
        OnComplete = "Player1"
      }
    },
    Jennifer = {
      {
        tAnims = {
          Player1 = "OIL02_Contract_Briefing_No_Jennifer",
          Starter = "OIL02_Contract_Briefing_No_Starter-Jennifer"
        },
        OnComplete = "Player1"
      }
    },
    Mattias = {
      {
        tAnims = {
          Player1 = "OIL02_Contract_Briefing_No_Mattias",
          Starter = "OIL02_Contract_Briefing_No_Starter-Mattias"
        },
        OnComplete = "Player1"
      }
    },
    tCameraEffects = {
      Chris = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2763,
            1.3514,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Jennifer = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2086,
            2.7981,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Mattias = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.448,
            2.3577,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      }
    }
  },
  tConfirmCinematic = {
    Chris = {
      {
        tAnims = {
          Player1 = "OIL02_Contract_Briefing_Yes_Chris",
          Starter = "OIL02_Contract_Briefing_Yes_Starter-Chris"
        },
        OnComplete = "Player1"
      }
    },
    Jennifer = {
      {
        tAnims = {
          Player1 = "OIL02_Contract_Briefing_Yes_Jennifer",
          Starter = "OIL02_Contract_Briefing_Yes_Starter-Jennifer"
        },
        OnComplete = "Player1"
      }
    },
    Mattias = {
      {
        tAnims = {
          Player1 = "OIL02_Contract_Briefing_Yes_Mattias",
          Starter = "OIL02_Contract_Briefing_Yes_Starter-Mattias"
        },
        OnComplete = "Player1"
      }
    },
    tCameraEffects = {
      Chris = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2763,
            1.3514,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Jennifer = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.2086,
            2.7981,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Mattias = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.448,
            2.3577,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      }
    }
  }
}
PmcCon002 = {
  nType = knSimple,
  tAssetPreload = {
    soundbank = {
      "vo_pmcCon002"
    },
    wavebank = {
      "vo_pmcCon002"
    }
  },
  tPositions = {
    Player1 = "hp_pmc02_player",
    Starter = "hp_pmc02_starter"
  },
  tFaceAnimSets = {
    Player1 = {
      Chris = "PMC02_Contract_Briefing_Chris",
      Jennifer = "PMC02_Contract_Briefing_Jennifer",
      Mattias = "PMC02_Contract_Briefing_Mattias"
    },
    Starter = "PMC02_Contract_Briefing_Starter"
  },
  tCinematic = {
    Chris = {
      {
        tAnims = {
          Player1 = "PMC02_Contract_Briefing_Chris",
          Starter = "PMC02_Contract_Briefing_Starter-Chris"
        },
        OnTime = 12.9667,
        OnComplete = "Player1"
      },
      {
        tFlash = {
          sFile = "PmcCon002_briefing.gfx",
          nTime = 19.0333
        },
        OnTime = 36.333298
      },
      {
        tCamera = {bHold = true},
        Stall = true
      }
    },
    Jennifer = {
      {
        tAnims = {
          Player1 = "PMC02_Contract_Briefing_Jennifer",
          Starter = "PMC02_Contract_Briefing_Starter-Jennifer"
        },
        OnTime = 8.0667,
        OnComplete = "Player1"
      },
      {
        tFlash = {
          sFile = "PmcCon002_briefing.gfx",
          nTime = 18.9
        },
        OnTime = 36.1
      },
      {
        tCamera = {bHold = true},
        Stall = true
      }
    },
    Mattias = {
      {
        tAnims = {
          Player1 = "PMC02_Contract_Briefing_Mattias",
          Starter = "PMC02_Contract_Briefing_Starter-Mattias"
        },
        OnTime = 8.9,
        OnComplete = "Player1"
      },
      {
        tFlash = {
          sFile = "PmcCon002_briefing.gfx",
          nTime = 20.1667
        },
        OnTime = 38.1
      },
      {
        tCamera = {bHold = true},
        Stall = true
      }
    },
    tCameraEffects = {
      Chris = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.0658,
            3.6543,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            65,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 3.23,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.0658,
            3.6543,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            65,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 8.53,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.0658,
            3.6543,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            65,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 32,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.0658,
            3.6543,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            65,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 33.8,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.0658,
            3.6543,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            65,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 35.53,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.0658,
            3.6543,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            65,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 39.5,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.0658,
            5.069,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            65,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 46.4,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.0658,
            3.7113,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Jennifer = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.0658,
            3.6543,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            65,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 3.33,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.0658,
            3.6543,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            65,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 5.57,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.0658,
            3.6543,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            65,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 26.97,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.0658,
            3.6543,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            65,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 29.07,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.0658,
            3.6543,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            65,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 31.07,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.0658,
            3.6543,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            65,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 34.47,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.0658,
            5.069,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            65,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 41.03,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.0658,
            3.7113,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Mattias = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.0658,
            3.6543,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 3,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.0658,
            3.6543,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 7.23,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.0658,
            3.6543,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 30.87,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.0658,
            3.6543,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 32.7,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.0658,
            3.6543,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 37.3,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.0658,
            5.069,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            65,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 43.83,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.0658,
            3.7113,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      }
    }
  },
  tDeclineCinematic = {
    Chris = {
      {
        tAnims = {
          Player1 = "PMC02_Contract_Briefing_No_Chris",
          Starter = "PMC02_Contract_Briefing_No_Starter-Chris"
        },
        OnComplete = "Player1"
      }
    },
    Jennifer = {
      {
        tAnims = {
          Player1 = "PMC02_Contract_Briefing_No_Jennifer",
          Starter = "PMC02_Contract_Briefing_No_Starter-Jennifer"
        },
        OnComplete = "Player1"
      }
    },
    Mattias = {
      {
        tAnims = {
          Player1 = "PMC02_Contract_Briefing_No_Mattias",
          Starter = "PMC02_Contract_Briefing_No_Starter-Mattias"
        },
        OnComplete = "Player1"
      }
    },
    tCameraEffects = {
      Chris = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.0658,
            3.7113,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Jennifer = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.0658,
            3.6543,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Mattias = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.0658,
            3.7113,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      }
    }
  },
  tConfirmCinematic = {
    Chris = {
      {
        tAnims = {
          Player1 = "PMC02_Contract_Briefing_Yes_Chris",
          Starter = "PMC02_Contract_Briefing_Yes_Starter-Chris"
        },
        OnComplete = "Player1"
      }
    },
    Jennifer = {
      {
        tAnims = {
          Player1 = "PMC02_Contract_Briefing_Yes_Jennifer",
          Starter = "PMC02_Contract_Briefing_Yes_Starter-Jennifer"
        },
        OnComplete = "Player1"
      }
    },
    Mattias = {
      {
        tAnims = {
          Player1 = "PMC02_Contract_Briefing_Yes_Mattias",
          Starter = "PMC02_Contract_Briefing_Yes_Starter-Mattias"
        },
        OnComplete = "Player1"
      }
    },
    tCameraEffects = {
      Chris = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.0658,
            3.7113,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Jennifer = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.0658,
            3.6543,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Mattias = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.0658,
            3.7113,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      }
    }
  }
}
PmcCon003 = {
  nType = knSimple,
  tAssetPreload = {
    soundbank = {
      "vo_pmcCon003"
    },
    wavebank = {
      "vo_pmcCon003"
    }
  },
  tPositions = {
    Player1 = "hp_pmc03_player",
    Starter = "hp_pmc03_starter"
  },
  tFaceAnimSets = {
    Player1 = {
      Chris = "PMC03_Contract_Briefing_Chris",
      Jennifer = "PMC03_Contract_Briefing_Jennifer",
      Mattias = "PMC03_Contract_Briefing_Mattias"
    },
    Starter = "PMC03_Contract_Briefing_Starter"
  },
  tCinematic = {
    Chris = {
      {
        tAnims = {
          Player1 = "PMC03_Contract_Briefing_Chris",
          Starter = "PMC03_Contract_Briefing_Starter-Chris"
        },
        OnTime = 3.1667,
        OnComplete = "Player1"
      },
      {
        tFlash = {
          sFile = "PmcCon003_briefing.gfx",
          nTime = 12.8
        },
        OnTime = 19.8333
      },
      {
        tCamera = {bHold = true},
        Stall = true
      }
    },
    Jennifer = {
      {
        tAnims = {
          Player1 = "PMC03_Contract_Briefing_Jennifer",
          Starter = "PMC03_Contract_Briefing_Starter-Jennifer"
        },
        OnTime = 3.4,
        OnComplete = "Player1"
      },
      {
        tFlash = {
          sFile = "PmcCon003_briefing.gfx",
          nTime = 12.5333
        },
        OnTime = 25.1667
      },
      {
        tCamera = {bHold = true},
        Stall = true
      }
    },
    Mattias = {
      {
        tAnims = {
          Player1 = "PMC03_Contract_Briefing_Mattias",
          Starter = "PMC03_Contract_Briefing_Starter-Mattias"
        },
        OnTime = 3.2,
        OnComplete = "Player1"
      },
      {
        tFlash = {
          sFile = "PmcCon003_briefing.gfx",
          nTime = 12.2667
        },
        OnTime = 18.0667
      },
      {
        tCamera = {bHold = true},
        Stall = true
      }
    },
    tCameraEffects = {
      Chris = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.0173,
            3.6461,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 19.23,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.0173,
            3.6461,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 21.73,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.0173,
            3.6461,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Jennifer = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.0173,
            3.6461,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 19.07,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.0173,
            3.6461,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 27.5,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.0173,
            3.6461,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Mattias = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.0173,
            3.6461,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 18.73,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.0173,
            3.6461,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 20.2,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.0173,
            3.6461,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      }
    }
  },
  tDeclineCinematic = {
    Chris = {
      {
        tAnims = {
          Player1 = "PMC03_Contract_Briefing_No_Chris",
          Starter = "PMC03_Contract_Briefing_No_Starter-Chris"
        },
        OnComplete = "Player1"
      }
    },
    Jennifer = {
      {
        tAnims = {
          Player1 = "PMC03_Contract_Briefing_No_Jennifer",
          Starter = "PMC03_Contract_Briefing_No_Starter-Jennifer"
        },
        OnComplete = "Player1"
      }
    },
    Mattias = {
      {
        tAnims = {
          Player1 = "PMC03_Contract_Briefing_No_Mattias",
          Starter = "PMC03_Contract_Briefing_No_Starter-Mattias"
        },
        OnComplete = "Player1"
      }
    },
    tCameraEffects = {
      Chris = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.0173,
            3.6461,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Jennifer = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.0173,
            3.6461,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Mattias = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.0173,
            3.6461,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      }
    }
  },
  tConfirmCinematic = {
    Chris = {
      {
        tAnims = {
          Player1 = "PMC03_Contract_Briefing_Yes_Chris",
          Starter = "PMC03_Contract_Briefing_Yes_Starter-Chris"
        },
        OnComplete = "Player1"
      }
    },
    Jennifer = {
      {
        tAnims = {
          Player1 = "PMC03_Contract_Briefing_Yes_Jennifer",
          Starter = "PMC03_Contract_Briefing_Yes_Starter-Jennifer"
        },
        OnComplete = "Player1"
      }
    },
    Mattias = {
      {
        tAnims = {
          Player1 = "PMC03_Contract_Briefing_Yes_Mattias",
          Starter = "PMC03_Contract_Briefing_Yes_Starter-Mattias"
        },
        OnComplete = "Player1"
      }
    },
    tCameraEffects = {
      Chris = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.0173,
            3.6461,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Jennifer = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.0173,
            3.6461,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Mattias = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.0173,
            3.6461,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      }
    }
  }
}
JetCon001 = {
  tAssetPreload = {
    soundbank = {
      "vo_jetRec001"
    },
    wavebank = {
      "vo_jetRec001"
    }
  },
  tPositions = {Player1 = "hp_playera", Starter = "hp_starter"},
  tFaceAnimSets = {
    Player1 = {
      Chris = "JET01_Contract_Briefing_Chris",
      Jennifer = "JET01_Contract_Briefing_Jennifer",
      Mattias = "JET01_Contract_Briefing_Mattias"
    },
    Starter = "JET01_Contract_Briefing_Starter"
  },
  tCinematic = {
    Chris = {
      {
        tAnims = {
          Player1 = "JET01_Contract_Briefing_Chris",
          Starter = "JET01_Contract_Briefing_Starter-Chris"
        },
        OnTime = 120.7333,
        OnComplete = "Player1"
      },
      {
        tFlash = {
          sFile = "JetCon001_briefing.gfx",
          nTime = 23.0667
        },
        OnTime = 30.433403
      },
      {
        tCamera = {bHold = true},
        Stall = true
      }
    },
    Jennifer = {
      {
        tAnims = {
          Player1 = "JET01_Contract_Briefing_Jennifer",
          Starter = "JET01_Contract_Briefing_Starter-Jennifer"
        },
        OnTime = 123.8333,
        OnComplete = "Player1"
      },
      {
        tFlash = {
          sFile = "JetCon001_briefing.gfx",
          nTime = 25.5333
        },
        OnTime = 32.5
      },
      {
        tCamera = {bHold = true},
        Stall = true
      }
    },
    Mattias = {
      {
        tAnims = {
          Player1 = "JET01_Contract_Briefing_Mattias",
          Starter = "JET01_Contract_Briefing_Starter-Mattias"
        },
        OnTime = 129.3667,
        OnComplete = "Player1"
      },
      {
        tFlash = {
          sFile = "JetCon001_briefing.gfx",
          nTime = 27.8
        },
        OnTime = 37.4666
      },
      {
        tCamera = {bHold = true},
        Stall = true
      }
    },
    tCameraEffects = {
      Chris = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1956,
            4.5428,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 2.7,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1956,
            4.5428,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 12.07,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1956,
            4.5428,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 15.3,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1956,
            4.5428,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 21,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1956,
            4.5428,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 22.83,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1956,
            4.5428,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 29.03,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1956,
            4.5428,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 30.33,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1956,
            4.5428,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 31.57,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1956,
            4.5428,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 41.67,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1956,
            4.5428,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 44.37,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1956,
            4.5428,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 46.23,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1956,
            4.5428,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 47.4,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1956,
            4.5428,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 53.5,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1956,
            4.5428,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 58.4,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1956,
            4.5428,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 62.77,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1956,
            4.5428,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 63.4,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1956,
            4.5428,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 65.93,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1956,
            4.5428,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 67.17,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1956,
            4.5428,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 71.83,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1956,
            4.5428,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 72.83,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1956,
            4.5428,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 78.57,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1956,
            4.5428,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 82.77,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1956,
            4.5428,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 92.5,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1956,
            4.5428,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 98.57,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1956,
            4.5428,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 99.9,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1956,
            4.5428,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 101.37,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1956,
            4.5428,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 108.13,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1956,
            4.5428,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 109.57,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1956,
            4.5428,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 110.3,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1956,
            4.5428,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 118.37,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1956,
            4.5428,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 119.3,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1956,
            4.5428,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 138.37,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1956,
            4.5428,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Jennifer = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1855,
            4.1399,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 3.37,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1855,
            4.1399,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 14.7,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1855,
            4.1399,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 17.53,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1855,
            4.1399,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 23.2,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1855,
            4.1399,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 25.17,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1855,
            4.1399,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 31.2,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1855,
            4.1399,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 32.33,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1855,
            4.1399,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 33.87,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1855,
            4.1399,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 43.8,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1855,
            4.1399,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 49.7,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1855,
            4.1399,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 55.57,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1855,
            4.1399,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 60.6,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1855,
            4.1399,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 64.87,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1855,
            4.1399,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 66.03,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1855,
            4.1399,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 68,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1855,
            4.1399,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 69.37,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1855,
            4.1399,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 73.97,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1855,
            4.1399,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 75.27,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1855,
            4.1399,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 80.83,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1855,
            4.1399,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 84.87,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1855,
            4.1399,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 94.67,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1855,
            4.1399,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 100.73,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1855,
            4.1399,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 102.17,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1855,
            4.1399,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 103.8,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1855,
            4.1399,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 110.47,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1855,
            4.1399,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 111.93,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1855,
            4.1399,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 112.83,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1855,
            4.1399,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 121.1,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1855,
            4.1399,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 122.03,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1855,
            4.1399,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 142.7,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1855,
            4.1399,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Mattias = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.226,
            4.6725,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 4.3,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.226,
            4.6725,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 16.37,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.226,
            4.6725,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 18.73,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.226,
            4.6725,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 24.4,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.226,
            4.6725,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 26.7,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.226,
            4.6725,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 32.77,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.226,
            4.6725,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 34.03,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.226,
            4.6725,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 35.8,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.226,
            4.6725,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 45.73,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.226,
            4.6725,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 48.83,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.226,
            4.6725,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 51,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.226,
            4.6725,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 52.27,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.226,
            4.6725,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 58.23,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.226,
            4.6725,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 64.37,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.226,
            4.6725,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 68.5,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.226,
            4.6725,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 69.57,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.226,
            4.6725,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 71.93,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.226,
            4.6725,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 73.53,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.226,
            4.6725,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 77.83,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.226,
            4.6725,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 78.97,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.226,
            4.6725,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 84.43,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.226,
            4.6725,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 88.83,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.226,
            4.6725,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 98.87,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.226,
            4.6725,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 104.83,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.226,
            4.6725,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 106.17,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.226,
            4.6725,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 107.63,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.226,
            4.6725,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 114.3,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.226,
            4.6725,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 116.07,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.226,
            4.6725,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 117.07,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.226,
            4.6725,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 126.83,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.226,
            4.6725,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 127.87,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.226,
            4.6725,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      }
    }
  },
  tDeclineCinematic = {
    Chris = {
      {
        tAnims = {
          Player1 = "JET01_Contract_Briefing_No_Chris",
          Starter = "JET01_Contract_Briefing_No_Starter-Chris"
        },
        OnComplete = "Player1"
      }
    },
    Jennifer = {
      {
        tAnims = {
          Player1 = "JET01_Contract_Briefing_No_Jennifer",
          Starter = "JET01_Contract_Briefing_No_Starter-Jennifer"
        },
        OnComplete = "Player1"
      }
    },
    Mattias = {
      {
        tAnims = {
          Player1 = "JET01_Contract_Briefing_No_Mattias",
          Starter = "JET01_Contract_Briefing_No_Starter-Mattias"
        },
        OnComplete = "Player1"
      }
    },
    tCameraEffects = {
      Chris = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1956,
            4.5428,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Jennifer = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1855,
            4.1399,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Mattias = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.226,
            4.6725,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      }
    }
  },
  tConfirmCinematic = {
    Chris = {
      {
        tAnims = {
          Player1 = "JET01_Contract_Briefing_Yes_Chris",
          Starter = "JET01_Contract_Briefing_Yes_Starter-Chris"
        },
        OnComplete = "Player1"
      }
    },
    Jennifer = {
      {
        tAnims = {
          Player1 = "JET01_Contract_Briefing_Yes_Jennifer",
          Starter = "JET01_Contract_Briefing_Yes_Starter-Jennifer"
        },
        OnComplete = "Player1"
      }
    },
    Mattias = {
      {
        tAnims = {
          Player1 = "JET01_Contract_Briefing_Yes_Mattias",
          Starter = "JET01_Contract_Briefing_Yes_Starter-Mattias"
        },
        OnComplete = "Player1"
      }
    },
    tCameraEffects = {
      Chris = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1956,
            4.5428,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Jennifer = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1855,
            4.1399,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Mattias = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.226,
            4.6725,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      }
    }
  }
}
MecCon001 = {
  tAssetPreload = {
    soundbank = {
      "vo_mechRec001"
    },
    wavebank = {
      "vo_mechRec001"
    }
  },
  tPositions = {Player1 = "hp_playera", Starter = "hp_starter"},
  tFaceAnimSets = {
    Player1 = {
      Chris = "MEC01_Contract_Briefing_Chris",
      Jennifer = "MEC01_Contract_Briefing_Jennifer",
      Mattias = "MEC01_Contract_Briefing_Mattias"
    },
    Starter = "MEC01_Contract_Briefing_Starter"
  },
  tCinematic = {
    Chris = {
      {
        tAnims = {
          Player1 = "MEC01_Contract_Briefing_Chris",
          Starter = "MEC01_Contract_Briefing_Starter-Chris"
        },
        OnTime = 67.8667,
        OnComplete = "Player1"
      },
      {
        tFlash = {
          sFile = "MecCon001_briefing.gfx",
          nTime = 24.0333
        },
        OnTime = 35.666603
      },
      {
        tCamera = {bHold = true},
        Stall = true
      }
    },
    Jennifer = {
      {
        tAnims = {
          Player1 = "MEC01_Contract_Briefing_Jennifer",
          Starter = "MEC01_Contract_Briefing_Starter-Jennifer"
        },
        OnTime = 69.4667,
        OnComplete = "Player1"
      },
      {
        tFlash = {
          sFile = "MecCon001_briefing.gfx",
          nTime = 23.4
        },
        OnTime = 36
      },
      {
        tCamera = {bHold = true},
        Stall = true
      }
    },
    Mattias = {
      {
        tAnims = {
          Player1 = "MEC01_Contract_Briefing_Mattias",
          Starter = "MEC01_Contract_Briefing_Starter-Mattias"
        },
        OnTime = 77.3667,
        OnComplete = "Player1"
      },
      {
        tFlash = {
          sFile = "MecCon001_briefing.gfx",
          nTime = 24.3667
        },
        OnTime = 36.666603
      },
      {
        tCamera = {bHold = true},
        Stall = true
      }
    },
    tCameraEffects = {
      Chris = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 1.03,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 3.03,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 5.4,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 8.63,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 12.6,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 18.27,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 25.7,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 30.23,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 32.47,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 36.03,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 41.3,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 50.03,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 53.37,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 56.37,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 58.53,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 62.9,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 64.13,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 65.7,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 93.87,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 99.93,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Jennifer = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 1.87,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 3.77,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 6.2,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 9.67,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 14.13,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 19.67,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 27.17,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 31.63,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 33.47,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 36.8,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 41.83,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 50.87,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 54.07,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 56.7,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 59.1,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 63.93,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 65.2,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 67.17,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 95.53,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 101.4,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Mattias = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 0.97,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 3.13,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 5.53,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 9.23,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 15.17,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 20.83,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 28.17,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 33.13,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 35.37,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 39.73,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 44.97,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 56.3,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 60.2,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 62.8,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 64.67,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 71,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 72.33,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 74.87,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 104.3,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        },
        {
          nTime = 110.1,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      }
    }
  },
  tDeclineCinematic = {
    Chris = {
      {
        tAnims = {
          Player1 = "MEC01_Contract_Briefing_No_Chris",
          Starter = "MEC01_Contract_Briefing_No_Starter-Chris"
        },
        OnComplete = "Player1"
      }
    },
    Jennifer = {
      {
        tAnims = {
          Player1 = "MEC01_Contract_Briefing_No_Jennifer",
          Starter = "MEC01_Contract_Briefing_No_Starter-Jennifer"
        },
        OnComplete = "Player1"
      }
    },
    Mattias = {
      {
        tAnims = {
          Player1 = "MEC01_Contract_Briefing_No_Mattias",
          Starter = "MEC01_Contract_Briefing_No_Starter-Mattias"
        },
        OnComplete = "Player1"
      }
    },
    tCameraEffects = {
      Chris = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Jennifer = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Mattias = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      }
    }
  },
  tConfirmCinematic = {
    Chris = {
      {
        tAnims = {
          Player1 = "MEC01_Contract_Briefing_Yes_Chris",
          Starter = "MEC01_Contract_Briefing_Yes_Starter-Chris"
        },
        OnComplete = "Player1"
      }
    },
    Jennifer = {
      {
        tAnims = {
          Player1 = "MEC01_Contract_Briefing_Yes_Jennifer",
          Starter = "MEC01_Contract_Briefing_Yes_Starter-Jennifer"
        },
        OnComplete = "Player1"
      }
    },
    Mattias = {
      {
        tAnims = {
          Player1 = "MEC01_Contract_Briefing_Yes_Mattias",
          Starter = "MEC01_Contract_Briefing_Yes_Starter-Mattias"
        },
        OnComplete = "Player1"
      }
    },
    tCameraEffects = {
      Chris = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Jennifer = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      },
      Mattias = {
        {
          nTime = 0,
          tDepthOfField = {
            nDuration,
            nAngle,
            nStartNear,
            0.1595,
            4.2963,
            nEndFar,
            nBlur
          },
          tFieldOfView = {
            nDuration,
            55,
            nStartNear,
            nEndNear,
            nStartFar,
            nEndFar,
            nBlur
          }
        }
      }
    }
  }
}
OilCon021 = {
  nType = knSimple,
  tAssetPreload = {
    soundbank = {
      "vo_oilCon021"
    },
    wavebank = {
      "vo_oilCon021"
    }
  }
}
