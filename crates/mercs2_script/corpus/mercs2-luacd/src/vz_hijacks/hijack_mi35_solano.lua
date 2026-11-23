import("MrxActionHijack")
import("MrxCinematic")
import("HijackContractManager")
import("MrxMusic")
import("MrxUtil")
import("MrxVerifyManager")
import("MrxSoundBanks")
local nSection = {}
local door_anim_A = "All_mi35_drivervehicle_SOLANOactionhijack_getin_section01_fb"
local door_anim_AFail = "All_mi35_drivervehicle_SOLANOactionhijack_getin_section01failure_fb"
local door_anim_B = "All_mi35_drivervehicle_SOLANOactionhijack_getin_section02_fb"
local door_anim_BFail = "All_mi35_drivervehicle_SOLANOactionhijack_getin_section02failure_fb"
local door_anim_C = "All_mi35_drivervehicle_SOLANOactionhijack_getin_section03_fb"
local door_anim_CFail = "All_mi35_drivervehicle_SOLANOactionhijack_getin_section03failure_fb"
local door_anim_D = "All_mi35_drivervehicle_SOLANOactionhijack_getin_section04_fb"
local door_anim_DFail = "All_mi35_drivervehicle_SOLANOactionhijack_getin_section04failure_fb"
local door_anim_E = "All_mi35_drivervehicle_SOLANOactionhijack_getin_section05_fb"
local door_anim_EFail = "All_mi35_drivervehicle_SOLANOactionhijack_getin_section05failure_fb"
local door_anim_F = "All_mi35_drivervehicle_SOLANOactionhijack_getin_section06_fb"
local door_anim_FFail = "All_mi35_drivervehicle_SOLANOactionhijack_getin_section06failure_fb"
local door_anim_G = "All_mi35_drivervehicle_SOLANOactionhijack_getin_section07_fb"
local door_anim_GFail = "All_mi35_drivervehicle_SOLANOactionhijack_getin_section07failure_fb"
local door_anim_H = "All_mi35_drivervehicle_SOLANOactionhijack_getin_section08_fb"
local door_anim_HFail = "All_mi35_drivervehicle_SOLANOactionhijack_getin_section07failure_fb"
local door_anim_I = "All_mi35_drivervehicle_SOLANOactionhijack_getin_section09_fb"
local door_anim_J = "All_mi35_drivervehicle_SOLANOactionhijack_getin_section10_fb"
local door_anim_K = "All_mi35_drivervehicle_SOLANOactionhijack_getin_section11_fb"
local uGunTemplate, uGunInstance, uRifleTemplate, uRifleInstance, chopper, bodyguard, gunner, specialEnding, Hijacker, Hijackee, HijackedVehicle

function Init()
  print("Hijack_Mi35_Solano: Loading vehicle animation assets")
  local assetType = "animation"
  Pg.LoadAsset(door_anim_A, assetType)
  Pg.LoadAsset(door_anim_B, assetType)
  Pg.LoadAsset(door_anim_C, assetType)
  Pg.LoadAsset(door_anim_D, assetType)
  Pg.LoadAsset(door_anim_DFail, assetType)
  Pg.LoadAsset(door_anim_E, assetType)
  Pg.LoadAsset(door_anim_EFail, assetType)
  Pg.LoadAsset(door_anim_F, assetType)
  Pg.LoadAsset(door_anim_FFail, assetType)
  Pg.LoadAsset(door_anim_G, assetType)
  Pg.LoadAsset(door_anim_GFail, assetType)
  Pg.LoadAsset(door_anim_H, assetType)
  Pg.LoadAsset(door_anim_HFail, assetType)
  Pg.LoadAsset(door_anim_I, assetType)
  Pg.LoadAsset(door_anim_J, assetType)
  Pg.LoadAsset(door_anim_K, assetType)
  Pg.LoadAsset("Global_weapon_pistol", "model")
  Pg.LoadAsset("global_weapon_ak47", "model")
  Pg.LoadAsset("_global_veh_ruin_transport_fire", "model")
  Pg.LoadAsset("PMC04_Solano_ActionHijack_Mattias", "facefxanimationset")
  Pg.LoadAsset("PMC04_Solano_ActionHijack_Chris", "facefxanimationset")
  Pg.LoadAsset("PMC04_Solano_ActionHijack_Jennifer", "facefxanimationset")
  Pg.LoadAsset("PMC04_Solano_ActionHijack_Solano", "facefxanimationset")
  Pg.LoadAsset("vz_hum_solano", "facefxactor")
  MrxSoundBanks.RequestAmbienceBank("vo_solanoAHJ")
end

function Deinit()
  print("Hijack_Mi35_Solano: Unloading vehicle animation assets")
  local assetType = "animation"
  Pg.UnloadAsset(door_anim_A, assetType)
  Pg.UnloadAsset(door_anim_B, assetType)
  Pg.UnloadAsset(door_anim_C, assetType)
  Pg.UnloadAsset(door_anim_D, assetType)
  Pg.UnloadAsset(door_anim_DFail, assetType)
  Pg.UnloadAsset(door_anim_E, assetType)
  Pg.UnloadAsset(door_anim_EFail, assetType)
  Pg.UnloadAsset(door_anim_F, assetType)
  Pg.UnloadAsset(door_anim_FFail, assetType)
  Pg.UnloadAsset(door_anim_G, assetType)
  Pg.UnloadAsset(door_anim_GFail, assetType)
  Pg.UnloadAsset(door_anim_H, assetType)
  Pg.UnloadAsset(door_anim_HFail, assetType)
  Pg.UnloadAsset(door_anim_I, assetType)
  Pg.UnloadAsset(door_anim_J, assetType)
  Pg.UnloadAsset(door_anim_K, assetType)
  Pg.UnloadAsset("Global_weapon_pistol", "model")
  Pg.UnloadAsset("global_weapon_ak47", "model")
  Pg.UnloadAsset("_global_veh_ruin_transport_fire", "model")
  Pg.UnloadAsset("PMC04_Solano_ActionHijack_Mattias", "facefxanimationset")
  Pg.UnloadAsset("PMC04_Solano_ActionHijack_Chris", "facefxanimationset")
  Pg.UnloadAsset("PMC04_Solano_ActionHijack_Jennifer", "facefxanimationset")
  Pg.UnloadAsset("PMC04_Solano_ActionHijack_Solano", "facefxanimationset")
  Pg.UnloadAsset("vz_hum_solano", "facefxactor")
end

function StartHijack(hijackerObject, hijackeeObject, seatObject, vehicleObject)
  Debug.Printf("********************* Hijack_Mi35_Solano: hijackerObject    ")
  Debug.Printf(hijackerObject)
  MrxActionHijack.RULESET_SOLANO = true
  Debug.Printf("********************* Hijack_Mi35_Solano: MrxActionHijack.RULESET_SOLANO")
  Debug.Printf(MrxActionHijack.RULESET_SOLANO)
  local bResult, sBone
  uGunTemplate = Pg.GetGuidByName("Action Hijack Prop (Pistol)")
  sBone = "bone_pistol"
  bResult, uGunInstance = Object.Attach(vehicleObject, sBone, uGunTemplate)
  print("Solano MI35: attaching pistol: " .. tostring(bResult) .. "  " .. tostring(uGunInstance))
  uRifleTemplate = Pg.GetGuidByName("Action Hijack Prop (Rifle)")
  sBone = "bone_ak47"
  bResult, uRifleInstance = Object.Attach(vehicleObject, sBone, uRifleTemplate)
  print("Solano MI35: attaching ak47: " .. tostring(bResult) .. "  " .. tostring(uRifleInstance))
  _THIS:Initialize(nil, hijackerObject, hijackeeObject, seatObject, vehicleObject)
  return true
end

function Initialize(mModule, self, hijackerObject, hijackeeObject, seatObject, vehicleObject)
  chopper = Pg.GetGuidByName("Mi35 (Solano Hijack)")
  bodyguard = Vehicle.GetRiders(chopper, "p")
  gunner = Vehicle.GetRiders(chopper, "c")
  self = self or {}
  ExtraActors = ExtraActors or {}
  self._hijacker = hijackerObject
  self._hijackee = hijackeeObject
  self._ActorOne = bodyguard[1]
  self._ActorTwo = gunner[1]
  self._seat = seatObject
  self._vehicle = vehicleObject
  self._OnActionHijackComplete = EndContract
  self._OnFailureAnimationComplete = CancelContract
  Hijackee = hijackeeObject
  Hijacker = hijackerObject
  HijackedVehicle = vehicleObject
  setmetatable(self, {__index = mModule})
  self._hijackerPlayer = Object.IsPlayerControlled(self._hijacker)
  local HeroName = MrxUtil.GetPrimaryCharacterName()
  print("Solano MI35: Player Character: " .. tostring(HeroName))
  local tMusic = {
    a = {
      Time = 0,
      Music = "mu_mission_pmccon004_01"
    },
    b = {
      Time = 1.53,
      Music = "mu_mission_pmccon004_02"
    },
    c = {
      Time = 4.23,
      Music = "mu_mission_pmccon004_03"
    },
    d = {
      Time = 12.13,
      Music = "mu_mission_pmccon004_04"
    }
  }
  local tSoundFx = {
    a = {
      Time = 0,
      Cue = "solanoAHJ_copterStarts"
    },
    b = {
      Time = 0,
      Cue = "solanoAHJ_copter_lp"
    },
    c = {
      Time = 0.86,
      Cue = "solanoAHJ_copter_doorClose"
    },
    d = {
      Time = 2,
      Cue = "solanoAHJ_copter_spinExt"
    },
    e = {
      Time = 5.03,
      Cue = "solanoAHJ_copter_spinInt"
    },
    f = {
      Time = 5.03,
      Cue = "solanoAHJ_copter_alarmInt_lp"
    },
    g = {
      Time = 5.8,
      Cue = "solanoAHJ_copter_spinExt_crazy"
    },
    h = {
      Time = 7.66,
      Cue = "solanoAHJ_exp"
    }
  }
  local tActorVo = {
    Mattias = {
      ActorFaceAnim = "PMC04_Solano_ActionHijack_Mattias",
      a = {
        Time = 9.43,
        Cue = "Mattias-In-Mission-Contract-Pmc04-12"
      },
      b = {
        Time = 13.86,
        Cue = "Mattias-In-Mission-Contract-Pmc04-16"
      },
      c = {
        Time = 0,
        Cue = "Mattias-In-Mission-Contract-Pmc04-45"
      },
      d = {
        Time = 1.63,
        Cue = "Mattias-In-Mission-Contract-Pmc04-33"
      },
      e = {
        Time = 13.73,
        Cue = "Mattias-In-Mission-Contract-Pmc04-48"
      },
      f = {
        Time = 13.73,
        Cue = "Mattias-In-Mission-Contract-Pmc04-51"
      },
      g = {
        Time = 14.6,
        Cue = "Mattias-In-Mission-Contract-Pmc04-54"
      },
      h = {
        Time = 0,
        Cue = "Mattias-In-Mission-Contract-Pmc04-45"
      },
      i = {
        Time = 12.8,
        Cue = "Mattias-In-Mission-Contract-Pmc04-48"
      },
      j = {
        Time = 8.8,
        Cue = "Mattias-In-Mission-Contract-Pmc04-51"
      },
      k = {
        Time = 13.67,
        Cue = "Mattias-In-Mission-Contract-Pmc04-57"
      },
      l = {
        Time = 15.5,
        Cue = "Mattias-In-Mission-Contract-Pmc04-60"
      }
    },
    Chris = {
      ActorFaceAnim = "PMC04_Solano_ActionHijack_Chris",
      a = {
        Time = 9.43,
        Cue = "Chris-In-Mission-Contract-Pmc04-14"
      },
      b = {
        Time = 13.86,
        Cue = "Chris-In-Mission-Contract-Pmc04-18"
      },
      c = {
        Time = 0,
        Cue = "Chris-In-Mission-Contract-Pmc04-47"
      },
      d = {
        Time = 1.63,
        Cue = "Chris-In-Mission-Contract-Pmc04-35"
      },
      e = {
        Time = 13.73,
        Cue = "Chris-In-Mission-Contract-Pmc04-50"
      },
      f = {
        Time = 13.73,
        Cue = "Chris-In-Mission-Contract-Pmc04-53"
      },
      g = {
        Time = 14.6,
        Cue = "Chris-In-Mission-Contract-Pmc04-56"
      },
      h = {
        Time = 0,
        Cue = "Chris-In-Mission-Contract-Pmc04-47"
      },
      i = {
        Time = 12.8,
        Cue = "Chris-In-Mission-Contract-Pmc04-50"
      },
      j = {
        Time = 8.8,
        Cue = "Chris-In-Mission-Contract-Pmc04-53"
      },
      k = {
        Time = 13.67,
        Cue = "Chris-In-Mission-Contract-Pmc04-59"
      },
      l = {
        Time = 15.5,
        Cue = "Chris-In-Mission-Contract-Pmc04-62"
      }
    },
    Jennifer = {
      ActorFaceAnim = "PMC04_Solano_ActionHijack_Jennifer",
      a = {
        Time = 9.43,
        Cue = "Jennifer-In-Mission-Contract-Pmc04-13"
      },
      b = {
        Time = 13.86,
        Cue = "Jennifer-In-Mission-Contract-Pmc04-17"
      },
      c = {
        Time = 0,
        Cue = "Jennifer-In-Mission-Contract-Pmc04-46"
      },
      d = {
        Time = 1.63,
        Cue = "Jennifer-In-Mission-Contract-Pmc04-34"
      },
      e = {
        Time = 13.73,
        Cue = "Jennifer-In-Mission-Contract-Pmc04-49"
      },
      f = {
        Time = 13.73,
        Cue = "Jennifer-In-Mission-Contract-Pmc04-52"
      },
      g = {
        Time = 14.6,
        Cue = "Jennifer-In-Mission-Contract-Pmc04-55"
      },
      h = {
        Time = 0,
        Cue = "Jennifer-In-Mission-Contract-Pmc04-46"
      },
      i = {
        Time = 12.8,
        Cue = "Jennifer-In-Mission-Contract-Pmc04-49"
      },
      j = {
        Time = 8.8,
        Cue = "Jennifer-In-Mission-Contract-Pmc04-52"
      },
      k = {
        Time = 13.67,
        Cue = "Jennifer-In-Mission-Contract-Pmc04-58"
      },
      l = {
        Time = 15.5,
        Cue = "Jennifer-In-Mission-Contract-Pmc04-61"
      }
    },
    Hijackee = {
      ActorFaceAnim = "PMC04_Solano_ActionHijack_Solano",
      a = {
        Time = 2.13,
        Cue = "Solano-In-Mission-Contract-Pmc04-11"
      },
      b = {
        Time = 10.76,
        Cue = "Solano-In-Mission-Contract-Pmc04-15"
      },
      c = {
        Time = 14.86,
        Cue = "Solano-In-Mission-Contract-Pmc04-19"
      }
    },
    Joyce = {
      a = {
        Time = 11.53,
        Cue = "Joyce-In-Mission-Contract-Pmc04-36"
      }
    },
    Peng = {
      a = {
        Time = 11.53,
        Cue = "Peng-In-Mission-Contract-Pmc04-37"
      }
    }
  }
  tFaceAnimSets = {
    ActorOne = {
      ActorGuid = self._hijacker,
      ActorAnim = tActorVo[HeroName].ActorFaceAnim
    },
    ActorTwo = {
      ActorGuid = self._hijackee,
      ActorAnim = tActorVo.Hijackee.ActorFaceAnim
    }
  }
  MrxActionHijack.InitializeActionHijack(self)
  self[1] = {
    hijackerAnimation = "ActionHijackHijackerA",
    hijackeeAnimation = "ActionHijackHijackeeA",
    ExtraActors = {
      ActorOne = "ActionHijackHijackee2A",
      ActorTwo = "ActionHijackHijackee3A",
      ActorOneAnimationFail = "ActionHijackHijackee2FailA",
      ActorTwoAnimationFail = "ActionHijackHijackee3FailA"
    },
    vehicleAnimation = door_anim_A,
    vehicleAnimationFail = door_anim_AFail,
    tMultiEvents = {
      {
        nTime = 0,
        tCameraParams = {
          nEndNear = 0,
          nStartFar = 7.1668,
          nFoV = 49
        }
      },
      {
        nTime = tSoundFx.a.Time,
        tPlaySoundFx = {
          nSoundFxName = tSoundFx.a.Cue
        }
      },
      {
        nTime = tMusic.a.Time,
        tPlayMusic = {
          nMusicName = tMusic.a.Music
        }
      }
    }
  }
  self[2] = {
    hijackerAnimation = "ActionHijackHijackerB",
    hijackeeAnimation = "ActionHijackHijackeeB",
    ExtraActors = {
      ActorOne = "ActionHijackHijackee2B",
      ActorTwo = "ActionHijackHijackee3B",
      ActorOneAnimationFail = "ActionHijackHijackee2FailB",
      ActorTwoAnimationFail = "ActionHijackHijackee3FailB"
    },
    vehicleAnimation = door_anim_B,
    vehicleAnimationFail = door_anim_BFail,
    tMultiEvents = {
      {
        nTime = 0,
        tCameraParams = {
          nEndNear = 0,
          nStartFar = 12.0851,
          nFoV = 45
        }
      },
      {
        nTime = tActorVo.Hijackee.a.Time,
        tPlaySingleVO = {
          nSpeaker = self._hijackee,
          nCueName = tActorVo.Hijackee.a.Cue
        }
      },
      {
        nTime = 5.33,
        tParticleEfx = {
          objectInstanceTemplate = self._hijackee,
          objectHardPointBoneName = "Bone_Attach_LHand",
          sPFXname = "global_particle_explosion_money_large"
        }
      }
    }
  }
  self[3] = {
    hijackerAnimation = "ActionHijackHijackerC",
    hijackeeAnimation = "ActionHijackHijackeeC",
    ExtraActors = {
      ActorOne = "ActionHijackHijackee2C",
      ActorTwo = "ActionHijackHijackee3C",
      ActorOneAnimationFail = "ActionHijackHijackee2FailC",
      ActorTwoAnimationFail = "ActionHijackHijackee3FailC"
    },
    vehicleAnimation = door_anim_C,
    vehicleAnimationFail = door_anim_CFail,
    tMultiEvents = {
      {
        nTime = 0,
        tCameraParams = {
          nEndNear = 0,
          nStartFar = 12.0851,
          nFoV = 43
        }
      }
    },
    OnAnimationComplete = OpenBunkerDoors(self)
  }
  self[4] = {
    hijackerAnimation = "ActionHijackHijackerD",
    hijackeeAnimation = "ActionHijackHijackeeD",
    hijackerAnimationFail = "ActionHijackHijackerFailD",
    hijackeeAnimationFail = "ActionHijackHijackeeFailD",
    ExtraActors = {
      ActorOne = "ActionHijackHijackee2D",
      ActorTwo = "ActionHijackHijackee3D",
      ActorOneAnimationFail = "ActionHijackHijackee2FailD"
    },
    vehicleAnimation = door_anim_D,
    vehicleAnimationFail = door_anim_DFail,
    tMultiEvents = {
      {
        nTime = 0,
        tCameraParams = {
          nEndNear = 0,
          nStartFar = 14.7199,
          nFoV = 45
        }
      },
      {
        nTime = tSoundFx.b.Time,
        tPlaySoundFx = {
          nSoundFxName = tSoundFx.b.Cue
        }
      },
      {
        nTime = 0.13,
        tControllerRumble = {nlength = 0.1},
        tCameraShake = {fSetCameraAmplitude = 1, fSetCameraShake = 0.1}
      },
      {
        nTime = tMusic.b.Time,
        tPlayMusic = {
          nMusicName = tMusic.b.Music
        }
      },
      {
        nTime = 2.36,
        tControllerRumble = {nlength = 0.5},
        tCameraShake = {fSetCameraAmplitude = 2.5, fSetCameraShake = 0.1}
      }
    },
    miniGameStartDelay = 0.1,
    miniGame = {
      nTimeOut = 2.5,
      sAction = "press",
      bExtraHudParameters = true,
      nTranslucency = 255,
      bShowTimer = false,
      nHudButtonMotionSpeed = 0.2,
      nXPosition = 0.5,
      nYPosition = 0.2,
      button = Controller.RPad_Right
    },
    OnFailureAnimationComplete = CancelContract
  }
  self[5] = {
    hijackerAnimation = "ActionHijackHijackerE",
    hijackeeAnimation = "ActionHijackHijackeeE",
    hijackerAnimationFail = "ActionHijackHijackerFailE",
    hijackeeAnimationFail = "ActionHijackHijackeeFailE",
    ExtraActors = {
      ActorOne = "ActionHijackHijackee2E",
      ActorTwo = "ActionHijackHijackee3E",
      ActorOneAnimationFail = "ActionHijackHijackee2FailE"
    },
    vehicleAnimation = door_anim_E,
    vehicleAnimationFail = door_anim_EFail,
    tMultiEvents = {
      {
        nTime = 0,
        tCameraParams = {
          nEndNear = 1.9239,
          nStartFar = 14.7199,
          nFoV = 45
        }
      },
      {
        nTime = 0.63,
        tControllerRumble = {nlength = 0.1},
        tCameraShake = {fSetCameraAmplitude = 1, fSetCameraShake = 0.1}
      },
      {
        nTime = 2.76,
        tControllerRumble = {nlength = 0.5},
        tCameraShake = {fSetCameraAmplitude = 2.5, fSetCameraShake = 0.1}
      }
    },
    miniGameStartDelay = 0.43,
    miniGame = {
      nTimeOut = 2.5,
      sAction = "press",
      sAction = "press",
      bExtraHudParameters = true,
      nTranslucency = 255,
      bShowTimer = false,
      nHudButtonMotionSpeed = 0.2,
      nXPosition = -0.55,
      nYPosition = 0.2,
      button = Controller.LStick_Left
    },
    OnFailureAnimationComplete = CancelContract,
    bActorOneDoneDead = true
  }
  self[6] = {
    hijackerAnimation = "ActionHijackHijackerF",
    hijackeeAnimation = "ActionHijackHijackeeF",
    hijackerAnimationFail = "ActionHijackHijackerFailF",
    hijackeeAnimationFail = "ActionHijackHijackeeFailF",
    ExtraActors = {
      ActorTwo = "ActionHijackHijackee3F",
      ActorTwoAnimationFail = "ActionHijackHijackee3FailF"
    },
    vehicleAnimation = door_anim_F,
    vehicleAnimationFail = door_anim_FFail,
    tMultiEvents = {
      {
        nTime = 0,
        tCameraParams = {
          nEndNear = 0,
          nStartFar = 14.7199,
          nFoV = 45
        }
      },
      {
        nTime = 1.5,
        tControllerRumble = {nlength = 0.1},
        tCameraShake = {fSetCameraAmplitude = 1, fSetCameraShake = 0.1}
      },
      {
        nTime = tSoundFx.c.Time,
        tPlaySoundFx = {
          nSoundFxName = tSoundFx.c.Cue
        }
      },
      {
        nTime = 4.4,
        tControllerRumble = {nlength = 0.5},
        tCameraShake = {fSetCameraAmplitude = 2.5, fSetCameraShake = 0.1}
      }
    },
    miniGameStartDelay = 3.97,
    miniGame = {
      nTimeOut = 2.5,
      sAction = "press",
      bExtraHudParameters = true,
      nTranslucency = 255,
      bShowTimer = false,
      nHudButtonMotionSpeed = 0.3,
      nXPosition = 0,
      nYPosition = 0.8,
      button = Controller.RPad_Down
    },
    OnFailureAnimationComplete = CancelContract
  }
  self[7] = {
    hijackerAnimation = "ActionHijackHijackerG",
    hijackeeAnimation = "ActionHijackHijackeeG",
    hijackerAnimationFail = "ActionHijackHijackerFailG",
    hijackeeAnimationFail = "ActionHijackHijackeeFailG",
    ExtraActors = {
      ActorTwo = "ActionHijackHijackee3G",
      ActorTwoAnimationFail = "ActionHijackHijackee3FailG"
    },
    vehicleAnimation = door_anim_G,
    vehicleAnimationFail = door_anim_GFail,
    tMultiEvents = {
      {
        nTime = 0,
        tCameraParams = {
          nEndNear = 0,
          nStartFar = 14.7199,
          nFoV = 45
        }
      },
      {
        nTime = 0.27,
        tControllerRumble = {nlength = 0.5},
        tCameraShake = {fSetCameraAmplitude = 2.5, fSetCameraShake = 0.1}
      }
    },
    miniGameStartDelay = 1.8,
    miniGame = {
      nTimeOut = 2.5,
      sAction = "press",
      bExtraHudParameters = true,
      nTranslucency = 255,
      bShowTimer = false,
      nHudButtonMotionSpeed = 0.3,
      nXPosition = 0.5,
      nYPosition = 0.2,
      button = Controller.LStick_Right
    },
    OnFailureAnimationComplete = CancelContract
  }
  self[8] = {
    hijackerAnimation = "ActionHijackHijackerH",
    hijackeeAnimation = "ActionHijackHijackeeH",
    hijackerAnimationFail = "ActionHijackHijackerFailH",
    hijackeeAnimationFail = "ActionHijackHijackeeFailH",
    ExtraActors = {
      ActorTwo = "ActionHijackHijackee3H",
      ActorTwoAnimationFail = "ActionHijackHijackee3FailH"
    },
    vehicleAnimation = door_anim_H,
    vehicleAnimationFail = door_anim_GFail,
    tMultiEvents = {
      {
        nTime = 0,
        tCameraParams = {
          nEndNear = 0,
          nStartFar = 14.7199,
          nFoV = 45
        }
      },
      {
        nTime = 0.23,
        tControllerRumble = {nlength = 0.1},
        tCameraShake = {fSetCameraAmplitude = 1, fSetCameraShake = 0.1}
      },
      {
        nTime = 0.83,
        tControllerRumble = {nlength = 0.3},
        tCameraShake = {fSetCameraAmplitude = 2, fSetCameraShake = 0.2}
      },
      {
        nTime = tSoundFx.d.Time,
        tPlaySoundFx = {
          nSoundFxName = tSoundFx.d.Cue
        }
      },
      {
        nTime = 2,
        tStopSoundFx = {
          nSoundFxName = tSoundFx.b.Cue
        }
      },
      {
        nTime = 6.13,
        tControllerRumble = {nlength = 0.5},
        tCameraShake = {fSetCameraAmplitude = 2.5, fSetCameraShake = 0.1}
      },
      {
        nTime = 2.7,
        tLinkObject = {
          vObject = "Light_solano_ahj",
          vAttachPoint = "hp_fx_light",
          vParent = self._vehicle,
          vState = true
        }
      }
    },
    miniGameStartDelay = 3.96,
    miniGame = {
      nTimeOut = 2.5,
      sAction = "press",
      bExtraHudParameters = true,
      nTranslucency = 255,
      bShowTimer = false,
      nHudButtonMotionSpeed = 0.3,
      nXPosition = -0.5,
      nYPosition = 0.2,
      button = Controller.RPad_Left
    },
    OnFailureAnimationComplete = CancelContract,
    bActorTwoDoneDead = true
  }
  self[9] = {
    hijackerAnimation = "ActionHijackHijackerI",
    hijackeeAnimation = "ActionHijackHijackeeI",
    vehicleAnimation = door_anim_I,
    tMultiEvents = {
      {
        nTime = 0,
        tCameraParams = {
          nEndNear = 0,
          nStartFar = 16.6208,
          nFoV = 45
        }
      },
      {
        nTime = 0.9667,
        tParticleEfx = {
          objectInstanceTemplate = uGunInstance,
          objectHardPointBoneName = "hp_barreltip_a",
          sPFXname = "global_particle_muzzleflash_handgun"
        }
      },
      {
        nTime = 0.9667,
        tParticleEfx = {
          objectInstanceTemplate = uGunInstance,
          objectHardPointBoneName = "hp_barreltip_a",
          sPFXname = "global_particle_shellhandgun"
        }
      },
      {
        nTime = 2,
        tControllerRumble = {nlength = 0.1},
        tCameraShake = {fSetCameraAmplitude = 1, fSetCameraShake = 0.1}
      },
      {
        nTime = 2.57,
        tControllerRumble = {nlength = 0.1},
        tCameraShake = {fSetCameraAmplitude = 1, fSetCameraShake = 0.1}
      },
      {
        nTime = tMusic.c.Time,
        tPlayMusic = {
          nMusicName = tMusic.c.Music
        }
      },
      {
        nTime = 4.73,
        tControllerRumble = {nlength = 0.5},
        tCameraShake = {fSetCameraAmplitude = 2.5, fSetCameraShake = 0.1}
      },
      {
        nTime = 5.03,
        tStopSoundFx = {
          nSoundFxName = tSoundFx.d.Cue
        }
      },
      {
        nTime = tSoundFx.e.Time,
        tPlaySoundFx = {
          nSoundFxName = tSoundFx.e.Cue
        }
      },
      {
        nTime = tSoundFx.f.Time,
        tPlaySoundFx = {
          nSoundFxName = tSoundFx.f.Cue
        }
      },
      {
        nTime = tActorVo[HeroName].a.Time,
        tPlaySingleVO = {
          nSpeaker = self._hijacker,
          nCueName = tActorVo[HeroName].a.Cue
        }
      },
      {
        nTime = tActorVo.Hijackee.b.Time,
        tPlaySingleVO = {
          nSpeaker = self._hijackee,
          nCueName = tActorVo.Hijackee.b.Cue
        }
      },
      {
        nTime = tActorVo[HeroName].b.Time,
        tPlaySingleVO = {
          nSpeaker = self._hijacker,
          nCueName = tActorVo[HeroName].b.Cue
        }
      },
      {
        nTime = tActorVo.Hijackee.c.Time,
        tPlaySingleVO = {
          nSpeaker = self._hijackee,
          nCueName = tActorVo.Hijackee.c.Cue
        }
      }
    }
  }
  specialEnding = false
  if MrxVerifyManager.GetKilled() == 0 then
    specialEnding = true
  end
  local HeroCue = tActorVo[HeroName].e.Cue
  local HeroCueTime = tActorVo[HeroName].e.Time
  local BossCue = tActorVo.Joyce.a.Cue
  local BossCueTime = tActorVo.Joyce.a.Time
  local Peng = false
  if Pg.GetGuidByName("ChinaNuked_Particle") then
    Peng = true
  end
  if Peng == true then
    HeroCue = tActorVo[HeroName].f.Cue
    HeroCueTime = tActorVo[HeroName].f.Time
    BossCue = tActorVo.Peng.a.Cue
    BossCueTime = tActorVo.Peng.a.Time
  end
  if specialEnding == false then
    self[10] = {
      hijackerAnimation = "ActionHijackHijackerJ",
      hijackeeAnimation = "ActionHijackHijackeeJ",
      vehicleAnimation = door_anim_J,
      tMultiEvents = {
        {
          nTime = 0,
          tCameraParams = {
            nEndNear = 0,
            nStartFar = 16.6208,
            nFoV = 45
          }
        },
        {
          nTime = tActorVo[HeroName].c.Time,
          tPlaySingleVO = {
            nSpeaker = self._hijacker,
            nCueName = tActorVo[HeroName].c.Cue
          }
        },
        {
          nTime = tActorVo[HeroName].d.Time,
          tPlaySingleVO = {
            nSpeaker = self._hijacker,
            nCueName = tActorVo[HeroName].d.Cue
          }
        },
        {
          nTime = 6.33,
          tLinkObject = {
            vObject = "Light_solano_ahj",
            vAttachPoint = "hp_fx_light",
            vParent = self._vehicle,
            vState = false
          }
        },
        {
          nTime = 7.73,
          tHeliKill = {
            tExplosionHP = Pg.GetGuidByName("loc_explosion4")
          }
        },
        {
          nTime = HeroCueTime,
          tPlaySingleVO = {
            nSpeaker = self._hijacker,
            nCueName = HeroCue
          }
        },
        {
          nTime = tActorVo[HeroName].g.Time,
          tPlaySingleVO = {
            nSpeaker = self._hijacker,
            nCueName = tActorVo[HeroName].g.Cue
          }
        },
        {
          nTime = 1.1,
          tParticleEfx = {
            objectInstanceTemplate = uGunInstance,
            objectHardPointBoneName = "hp_barreltip_a",
            sPFXname = "global_particle_muzzleflash_handgun"
          }
        },
        {
          nTime = 1.1,
          tParticleEfx = {
            objectInstanceTemplate = uGunInstance,
            objectHardPointBoneName = "hp_barreltip_a",
            sPFXname = "global_particle_shellhandgun"
          }
        },
        {
          nTime = 1.13,
          tControllerRumble = {nlength = 0.2},
          tParticleEfx = {
            objectInstanceTemplate = self._hijacker,
            objectHardPointBoneName = "Bone_LShoulder",
            sPFXname = "global_particle_impact_blood"
          }
        },
        {
          nTime = 5.76,
          tStopSoundFx = {
            nSoundFxName = tSoundFx.e.Cue
          }
        },
        {
          nTime = 5.76,
          tStopSoundFx = {
            nSoundFxName = tSoundFx.f.Cue
          }
        },
        {
          nTime = 5.8,
          tCameraParams = {nEndNear = 0, nStartFar = 29.807}
        },
        {
          nTime = 7.73,
          tControllerRumble = {nlength = 3},
          tCameraShake = {fSetCameraAmplitude = 20, fSetCameraShake = 3}
        },
        {
          nTime = tSoundFx.g.Time,
          tPlaySoundFx = {
            nSoundFxName = tSoundFx.g.Cue
          }
        },
        {
          nTime = 7.63,
          tStopSoundFx = {
            nSoundFxName = tSoundFx.g.Cue
          }
        },
        {
          nTime = tSoundFx.h.Time,
          tPlaySoundFx = {
            nSoundFxName = tSoundFx.h.Cue
          }
        },
        {
          nTime = tMusic.d.Time,
          tPlayMusic = {
            nMusicName = tMusic.d.Music
          }
        },
        {
          nTime = 20.7,
          tStopSoundFx = {
            nSoundFxName = tSoundFx.h.Cue
          }
        }
      },
      nFadeOut = {fInDelay = 20.93},
      OnActionHijackComplete = EndContract
    }
  else
    self[10] = {
      hijackerAnimation = "ActionHijackHijackerK",
      hijackeeAnimation = "ActionHijackHijackeeK",
      vehicleAnimation = door_anim_K,
      tMultiEvents = {
        {
          nTime = 0,
          tCameraParams = {
            nEndNear = 0,
            nStartFar = 16.6208,
            nFoV = 45
          }
        },
        {
          nTime = tActorVo[HeroName].h.Time,
          tPlaySingleVO = {
            nSpeaker = self._hijacker,
            nCueName = tActorVo[HeroName].h.Cue
          }
        },
        {
          nTime = 12.8,
          tPlaySingleVO = {
            nSpeaker = self._hijacker,
            nCueName = HeroCue
          }
        },
        {
          nTime = tActorVo[HeroName].k.Time,
          tPlaySingleVO = {
            nSpeaker = self._hijacker,
            nCueName = tActorVo[HeroName].k.Cue
          }
        },
        {
          nTime = tActorVo[HeroName].l.Time,
          tPlaySingleVO = {
            nSpeaker = self._hijacker,
            nCueName = tActorVo[HeroName].l.Cue
          }
        },
        {
          nTime = 4.73,
          tCameraParams = {nEndNear = 0, nStartFar = 29.807}
        },
        {
          nTime = 5.06,
          tLinkObject = {
            vObject = "Light_solano_ahj",
            vAttachPoint = "hp_fx_light",
            vParent = self._vehicle,
            vState = false
          }
        },
        {
          nTime = 5.76,
          tStopSoundFx = {
            nSoundFxName = tSoundFx.e.Cue
          }
        },
        {
          nTime = 5.76,
          tStopSoundFx = {
            nSoundFxName = tSoundFx.f.Cue
          }
        },
        {
          nTime = 6.56,
          tHeliKill = {
            tExplosionHP = Pg.GetGuidByName("loc_explosion4")
          }
        },
        {
          nTime = 6.56,
          tControllerRumble = {nlength = 3},
          tCameraShake = {fSetCameraAmplitude = 20, fSetCameraShake = 3}
        },
        {
          nTime = tSoundFx.g.Time,
          tPlaySoundFx = {
            nSoundFxName = tSoundFx.g.Cue
          }
        },
        {
          nTime = 7.63,
          tStopSoundFx = {
            nSoundFxName = tSoundFx.g.Cue
          }
        },
        {
          nTime = tSoundFx.h.Time,
          tPlaySoundFx = {
            nSoundFxName = tSoundFx.h.Cue
          }
        },
        {
          nTime = tMusic.d.Time,
          tPlayMusic = {
            nMusicName = tMusic.d.Music
          }
        },
        {
          nTime = 20.7,
          tStopSoundFx = {
            nSoundFxName = tSoundFx.h.Cue
          }
        }
      },
      nFadeOut = {fInDelay = 20.73},
      OnActionHijackComplete = EndContract
    }
  end
  MrxActionHijack.Begin(self, 1)
  return self
end

function EndContract()
  Event.Post("SolanoHijackComplete", {})
  Object.Remove(Hijackee)
  Event.Create(Event.ObjectInSeat, {
    Hijacker,
    HijackedVehicle,
    "a",
    "x"
  }, OnHijackerExit, {})
  Vehicle.Exit(HijackedVehicle, Hijacker, true)
  Debug.Printf("********************* Hijack_Mi35_Solano: Complete event posted")
end

function OnHijackerExit()
  Object.SetVisible(HijackedVehicle, false)
end

function CancelContract()
  Event.Post("SolanoHijackFailed", {})
  Debug.Printf("********************* Hijack_Mi35_Solano: Cancel event posted")
end

function ToggleLight(self, EventTime, EventState)
end

function RemoveWeapons(self)
  local bResult
  bResult = Object.Detach(self._vehicle, uGunInstance)
  Object.Remove(uGunInstance)
  print("Solano MI35: OnActionHijackComplete: remove gun: " .. tostring(bResult))
  bResult = Object.Detach(self._vehicle, uRifleInstance)
  Object.Remove(uRifleInstance)
  print("Solano MI35: OnActionHijackComplete: remove ak47: " .. tostring(bResult))
end

function BlackScreen()
  print("UNGST!!!")
  MrxCinematic.PlaceholderSequence({
    {sCaption = "YOU WIN!"},
    {
      sCaption = "You shot Solano in the face!"
    }
  }, AllDone)
end

function OpenBunkerDoors(self)
  local uBunkerDoorGuid = Pg.GetGuidByName("SolanoBunkerDoors")
  Object.OpenGate(uBunkerDoorGuid)
  print("@@@@@@@@@@ BUNKER DOORS OPENING!!!!!")
end

function AllDone()
  print("ALL DONE!")
end
