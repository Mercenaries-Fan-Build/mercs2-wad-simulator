import("MrxActionHijack")
local nSection = {}
local door_anim_A = "all_stingray_driverhatch_actionhijack_getin_section01_fb"
local door_anim_AFail = "all_stingray_driverhatch_actionhijack_getin_section01failure_fb"
local door_anim_B = "all_stingray_driverhatch_actionhijack_getin_section02_fb"
local door_anim_BFail = "all_stingray_driverhatch_actionhijack_getin_section02failure_fb"
local door_anim_C = "all_stingray_driverhatch_actionhijack_getin_section03_fb"
local door_anim_CFail = "all_stingray_driverhatch_actionhijack_getin_section03failure_fb"
local door_anim_D = "all_stingray_driverhatch_actionhijack_getin_section04_fb"
local door_anim_DLoop = "all_stingray_driverhatch_actionhijack_getin_section04loop_fb"
local door_anim_DLoopA = "all_stingray_driverhatch_actionhijack_getin_section04loopa_fb"
local door_anim_DLoopB = "all_stingray_driverhatch_actionhijack_getin_section04loopb_fb"
local door_anim_DLoopC = "all_stingray_driverhatch_actionhijack_getin_section04loopc_fb"
local door_anim_DLoopD = "all_stingray_driverhatch_actionhijack_getin_section04loopd_fb"
local door_anim_DLoopE = "all_stingray_driverhatch_actionhijack_getin_section04loope_fb"
local door_anim_DLoopF = "all_stingray_driverhatch_actionhijack_getin_section04loopf_fb"
local door_anim_DLoopG = "all_stingray_driverhatch_actionhijack_getin_section04loopg_fb"
local door_anim_DLoopH = "all_stingray_driverhatch_actionhijack_getin_section04looph_fb"
local door_anim_DLoopI = "all_stingray_driverhatch_actionhijack_getin_section04loopi_fb"
local door_anim_DLoopFail = "all_stingray_driverhatch_actionhijack_getin_section04loopfailure_fb"
local door_anim_E = "all_stingray_driverhatch_actionhijack_getin_section05_fb"
local door_anim_EFail = "all_stingray_driverhatch_actionhijack_getin_section05failure_fb"
local door_anim_F = "all_stingray_driverhatch_actionhijack_getin_section06_fb"
local uGunTemplate, uGunInstance, uGrenadeTemplate, uGrenadeInstance

function Init()
  local assetType = "animation"
  Pg.LoadAsset(door_anim_A, assetType)
  Pg.LoadAsset(door_anim_AFail, assetType)
  Pg.LoadAsset(door_anim_B, assetType)
  Pg.LoadAsset(door_anim_BFail, assetType)
  Pg.LoadAsset(door_anim_C, assetType)
  Pg.LoadAsset(door_anim_CFail, assetType)
  Pg.LoadAsset(door_anim_D, assetType)
  Pg.LoadAsset(door_anim_DLoop, assetType)
  Pg.LoadAsset(door_anim_DLoopA, assetType)
  Pg.LoadAsset(door_anim_DLoopB, assetType)
  Pg.LoadAsset(door_anim_DLoopC, assetType)
  Pg.LoadAsset(door_anim_DLoopD, assetType)
  Pg.LoadAsset(door_anim_DLoopE, assetType)
  Pg.LoadAsset(door_anim_DLoopF, assetType)
  Pg.LoadAsset(door_anim_DLoopG, assetType)
  Pg.LoadAsset(door_anim_DLoopH, assetType)
  Pg.LoadAsset(door_anim_DLoopI, assetType)
  Pg.LoadAsset(door_anim_DLoopFail, assetType)
  Pg.LoadAsset(door_anim_E, assetType)
  Pg.LoadAsset(door_anim_EFail, assetType)
  Pg.LoadAsset(door_anim_F, assetType)
  Pg.LoadAsset("Global_weapon_pistol", "model")
  Pg.LoadAsset("global_weapon_fraggrenade", "model")
end

function Deinit()
  local assetType = "animation"
  Pg.UnloadAsset(door_anim_A, assetType)
  Pg.UnloadAsset(door_anim_AFail, assetType)
  Pg.UnloadAsset(door_anim_B, assetType)
  Pg.UnloadAsset(door_anim_BFail, assetType)
  Pg.UnloadAsset(door_anim_C, assetType)
  Pg.UnloadAsset(door_anim_CFail, assetType)
  Pg.UnloadAsset(door_anim_D, assetType)
  Pg.UnloadAsset(door_anim_DLoop, assetType)
  Pg.UnloadAsset(door_anim_DLoopA, assetType)
  Pg.UnloadAsset(door_anim_DLoopB, assetType)
  Pg.UnloadAsset(door_anim_DLoopC, assetType)
  Pg.UnloadAsset(door_anim_DLoopD, assetType)
  Pg.UnloadAsset(door_anim_DLoopE, assetType)
  Pg.UnloadAsset(door_anim_DLoopF, assetType)
  Pg.UnloadAsset(door_anim_DLoopG, assetType)
  Pg.UnloadAsset(door_anim_DLoopH, assetType)
  Pg.UnloadAsset(door_anim_DLoopI, assetType)
  Pg.UnloadAsset(door_anim_DLoopFail, assetType)
  Pg.UnloadAsset(door_anim_E, assetType)
  Pg.UnloadAsset(door_anim_EFail, assetType)
  Pg.UnloadAsset(door_anim_F, assetType)
  Pg.UnloadAsset("Global_weapon_pistol", "model")
  Pg.UnloadAsset("global_weapon_fraggrenade", "model")
end

function StartHijack(hijackerObject, hijackeeObject, seatObject, vehicleObject)
  local bResult, sBone
  uGunTemplate = Pg.GetGuidByName("Action Hijack Prop (Pistol)")
  sBone = "bone_pistol"
  bResult, uGunInstance = Object.Attach(vehicleObject, sBone, uGunTemplate)
  Debug.Printf("StartHijack: attaching pistol: " .. tostring(bResult) .. "  " .. tostring(uGunInstance))
  uGrenadeTemplate = Pg.GetGuidByName("Action Hijack Prop (Grenade)")
  sBone = "bone_grenade"
  bResult, uGrenadeInstance = Object.Attach(vehicleObject, sBone, uGrenadeTemplate)
  Debug.Printf("StartHijack: attaching pistol: " .. tostring(bResult) .. "  " .. tostring(uGunInstance))
  _THIS:Initialize(nil, hijackerObject, hijackeeObject, seatObject, vehicleObject)
  return true
end

function Initialize(mModule, self, hijackerObject, hijackeeObject, seatObject, vehicleObject)
  self = self or {}
  tCharactersFaceStates = tCharactersFaceStates or {}
  self._hijacker = hijackerObject
  self._hijackee = hijackeeObject
  self._seat = seatObject
  self._vehicle = vehicleObject
  setmetatable(self, {__index = mModule})
  self._hijackerPlayer = Object.IsPlayerControlled(self._hijacker)
  self._OnActionHijackComplete = RemoveWeapons
  MrxActionHijack.InitializeActionHijack(self)
  FaceState_anim_BLoopA = {
    hijackee = {
      faceStateA = {state = "Scared", weight = 1}
    }
  }
  FaceState_anim_BLoopB = {
    hijackee = {
      faceStateA = {state = "Scared", weight = 0.75}
    }
  }
  FaceState_anim_BLoopC = {
    hijackee = {
      faceStateA = {state = "Scared", weight = 0.25}
    }
  }
  FaceState_anim_BLoopD = {
    hijackee = {
      faceStateA = {state = "Scared", weight = 0}
    }
  }
  FaceState_anim_BLoopF = {
    hijackee = {
      faceStateA = {state = "EyesShut", weight = 1}
    }
  }
  FaceState_anim_BLoopG = {
    hijackee = {
      faceStateA = {state = "angry", weight = 0.25}
    }
  }
  FaceState_anim_BLoopH = {
    hijackee = {
      faceStateA = {state = "angry", weight = 0.5}
    }
  }
  FaceState_anim_BLoopI = {
    hijackee = {
      faceStateA = {state = "angry", weight = 1}
    }
  }
  MrxActionHijack.TankPrep(self)
  self[1] = {
    hijackerAnimation = "ActionHijackHijackerA",
    hijackeeAnimation = "ActionHijackHijackeeA",
    vehicleAnimation = door_anim_A,
    tCharactersFaceStates = {
      hijacker = {
        faceStateA = {state = "angry", weight = 0.75}
      },
      hijackee = {
        faceStateA = {state = "angry", weight = 0.75}
      }
    },
    tMultiEvents = {
      {
        nTime = 0.13,
        tControllerRumble = {nlength = 0.2},
        tCameraShake = {fSetCameraAmplitude = 2, fSetCameraShake = 0.1}
      }
    }
  }
  self[2] = {
    hijackerAnimation = "ActionHijackHijackerB",
    hijackeeAnimation = "ActionHijackHijackeeB",
    hijackerAnimationFail = "ActionHijackHijackerFailB",
    hijackeeAnimationFail = "ActionHijackHijackeeFailB",
    vehicleAnimation = door_anim_B,
    vehicleAnimationFail = door_anim_BFail,
    tCharactersFaceStates = {
      hijacker = {
        faceStateA = {state = "angry", weight = 1},
        faceStateB = {state = "EyesShut", weight = 1}
      },
      hijackee = {
        faceStateA = {state = "angry", weight = 1}
      }
    },
    miniGameStartDelay = 1.06,
    miniGame = {
      nTimeOut = 1,
      bExtraHudParameters = true,
      nTranslucency = 255,
      nXPosition = 0,
      nYPosition = 0.8,
      bShowTimer = false,
      sAction = "press",
      nHudButtonMotionSpeed = 0.3,
      button = Controller.RPad_Down,
      nScale = 1.2
    },
    OnFailureAnimationBegin = OnFailureEvents,
    tFailureMultiEvents = {
      {
        nTime = 0.2,
        tControllerRumble = {nlength = 0.5},
        tCameraShake = {fSetCameraAmplitude = 2, fSetCameraShake = 0.1}
      }
    }
  }
  self[3] = {
    hijackerAnimation = "ActionHijackHijackerC",
    hijackeeAnimation = "ActionHijackHijackeeC",
    hijackerAnimationFail = "ActionHijackHijackerFailC",
    hijackeeAnimationFail = "ActionHijackHijackeeFailC",
    vehicleAnimation = door_anim_C,
    vehicleAnimationFail = door_anim_CFail,
    tCharactersFaceStates = {
      hijackee = {
        faceStateA = {state = "angry", weight = 1}
      },
      hijackeeFail = {
        faceStateA = {state = "angry", weight = 0},
        faceStateB = {state = "EyesShut", weight = 1}
      }
    },
    miniGameStartDelay = 2.86,
    miniGame = {
      nTimeOut = 1,
      bExtraHudParameters = true,
      nTranslucency = 255,
      nXPosition = 0,
      nYPosition = -0.45,
      bShowTimer = false,
      nHudButtonMotionSpeed = 0.2,
      sAction = "press",
      button = Controller.RPad_Up
    },
    tMultiEvents = {
      {
        nTime = 0.7,
        tControllerRumble = {nlength = 0.2},
        tCameraShake = {fSetCameraAmplitude = 1, fSetCameraShake = 0.1}
      },
      {
        nTime = 3.2,
        tControllerRumble = {nlength = 0.2},
        tCameraShake = {fSetCameraAmplitude = 2.5, fSetCameraShake = 0.1}
      }
    },
    OnFailureAnimationBegin = OnFailureEvents,
    tFailureMultiEvents = {
      {
        nTime = 0.23,
        tParticleEfx = {
          objectInstanceTemplate = uGunInstance,
          objectHardPointBoneName = "hp_barreltip_a",
          sPFXname = "global_particle_muzzleflash_handgun"
        },
        tControllerRumble = {nlength = 0.5},
        tCameraShake = {fSetCameraAmplitude = 1, fSetCameraShake = 0.1}
      },
      {
        nTime = 0.23,
        tParticleEfx = {
          objectInstanceTemplate = uGunInstance,
          objectHardPointBoneName = "hp_barreltip_a",
          sPFXname = "global_particle_shellhandgun"
        }
      },
      {
        nTime = 0.3,
        tControllerRumble = {nlength = 0.2},
        tParticleEfx = {
          objectInstanceTemplate = self._hijacker,
          objectHardPointBoneName = "Bone_LShoulder",
          sPFXname = "global_particle_impact_blood"
        }
      }
    }
  }
  self[4] = {
    hijackerAnimation = "ActionHijackHijackerD",
    hijackeeAnimation = "ActionHijackHijackeeD",
    vehicleAnimation = door_anim_D,
    tCharactersFaceStates = {
      hijackee = {
        faceStateA = {state = "angry", weight = 1}
      }
    },
    tMultiEvents = {
      {
        nTime = 0.76,
        tControllerRumble = {nlength = 0.2},
        tCameraShake = {fSetCameraAmplitude = 2, fSetCameraShake = 0.1}
      }
    }
  }
  nSection.ReactiveLoopA = 5
  self[5] = {
    hijackerAnimation = "ActionHijackHijackerDLoop",
    hijackeeAnimation = "ActionHijackHijackeeDLoop",
    hijackerAnimationFail = "ActionHijackHijackerFailD",
    hijackeeAnimationFail = "ActionHijackHijackeeFailD",
    tCharactersFaceStates = {
      hijackerFail = {
        faceStateA = {state = "angry", weight = 0},
        faceStateB = {state = "Scared", weight = 1}
      },
      hijackeeFail = {
        faceStateA = {state = "angry", weight = 1}
      }
    },
    vehicleAnimation = door_anim_DLoop,
    vehicleAnimationFail = door_anim_DLoopFail,
    nReactiveLoop = 2,
    tHijackerAnimations = {
      "ActionHijackHijackerDLoopI",
      "ActionHijackHijackerDLoopH",
      "ActionHijackHijackerDLoopG",
      "ActionHijackHijackerDLoopF",
      "ActionHijackHijackerDLoopD",
      "ActionHijackHijackerDLoopC",
      "ActionHijackHijackerDLoopB",
      "ActionHijackHijackerDLoopA"
    },
    tHijackeeAnimations = {
      "ActionHijackHijackeeBLoopI",
      "ActionHijackHijackeeBLoopH",
      "ActionHijackHijackeeBLoopG",
      "ActionHijackHijackeeBLoopF",
      "ActionHijackHijackeeBLoopD",
      "ActionHijackHijackeeBLoopC",
      "ActionHijackHijackeeBLoopB",
      "ActionHijackHijackeeBLoopA"
    },
    tVehicleAnimations = {
      door_anim_DLoopI,
      door_anim_DLoopH,
      door_anim_DLoopG,
      door_anim_DLoopF,
      door_anim_DLoopD,
      door_anim_DLoopC,
      door_anim_DLoopB,
      door_anim_DLoopA
    },
    tReactiveLoopFaceStates = {
      FaceState_anim_BLoopI,
      FaceState_anim_BLoopH,
      FaceState_anim_BLoopG,
      FaceState_anim_BLoopF,
      FaceState_anim_BLoopD,
      FaceState_anim_BLoopC,
      FaceState_anim_BLoopB,
      FaceState_anim_BLoopA
    },
    miniGameStartDelay = 0.01,
    miniGame = {
      nTimeOut = 35,
      bExtraHudParameters = true,
      nTranslucency = 255,
      nXPosition = -0.5,
      nYPosition = 0.2,
      bShowTimer = false,
      sAction = "tap",
      button = Controller.RPad_Left,
      nDriverDifficulty = 1.2,
      nSuccessThreshold = 1
    },
    OnFailureAnimationBegin = OnFailureEvents,
    tFailureMultiEvents = {
      {
        nTime = 0.5,
        tControllerRumble = {nlength = 0.5},
        tCameraShake = {fSetCameraAmplitude = 2, fSetCameraShake = 0.1}
      }
    }
  }
  self[6] = {
    hijackerAnimation = "ActionHijackHijackerE",
    hijackeeAnimation = "ActionHijackHijackeeE",
    vehicleAnimation = door_anim_E,
    tCharactersFaceStates = {
      hijackee = {
        faceStateA = {state = "Scared", weight = 1}
      }
    },
    tMultiEvents = {
      {
        nTime = 0.1,
        tControllerRumble = {nlength = 0.2},
        tCameraShake = {fSetCameraAmplitude = 2.5, fSetCameraShake = 0.1}
      }
    }
  }
  self[7] = {
    hijackerAnimation = "ActionHijackHijackerF",
    hijackeeAnimation = "ActionHijackHijackeeF",
    vehicleAnimation = door_anim_F,
    tCharactersFaceStates = {
      hijackee = {
        faceStateA = {state = "Scared", weight = 1}
      }
    },
    tMultiEvents = {
      {
        nTime = 0.2,
        tParticleEfx = {
          objectInstanceTemplate = uGunInstance,
          objectHardPointBoneName = "hp_barreltip_a",
          sPFXname = "global_particle_muzzleflash_handgun"
        },
        tControllerRumble = {nlength = 0.2},
        tCameraShake = {fSetCameraAmplitude = 1, fSetCameraShake = 0.1}
      },
      {
        nTime = 0.2333,
        tParticleEfx = {
          objectInstanceTemplate = uGunInstance,
          objectHardPointBoneName = "hp_barreltip_a",
          sPFXname = "global_particle_shellhandgun"
        }
      },
      {
        nTime = 0.3,
        tControllerRumble = {nlength = 0.2},
        tParticleEfx = {
          objectInstanceTemplate = self._hijackee,
          objectHardPointBoneName = "Bone_LShoulder",
          sPFXname = "global_particle_impact_blood"
        }
      },
      {
        nTime = 1.5,
        tControllerRumble = {nlength = 1},
        tParticleEfx = {
          objectInstanceTemplate = self._vehicle,
          objectHardPointBoneName = "hp_seat_lt",
          sPFXname = "global_particle_explosion_tankhatch"
        },
        tCameraShake = {fSetCameraAmplitude = 10, fSetCameraShake = 0.2}
      }
    },
    bDriverDoneRemove = true
  }
  MrxActionHijack.Begin(self, 1)
  return self
end

function RemoveWeapons(self)
  local bResult
  bResult = Object.Detach(self._vehicle, uGunInstance)
  Object.Remove(uGunInstance)
  Debug.Printf("M1A2: OnActionHijackComplete: remove gun: " .. tostring(bResult))
  bResult = Object.Detach(self._vehicle, uGrenadeInstance)
  Object.Remove(uGrenadeInstance)
  Debug.Printf("M1A2: OnActionHijackComplete: remove grenade: " .. tostring(bResult))
end

function OnFailureEvents(self, nCurrent)
  Debug.Printf("OnFailureAnimationBegin: nCurrent: " .. tostring(nCurrent))
  local tFailEvents = self[self.nCurrent].tFailureMultiEvents
  local oLocalPlayer = self._hijacker
  MrxActionHijack._ProcessMultiEventTable(oLocalPlayer, tFailEvents)
end
