import("MrxActionHijack")
local door_anim_A = "All_AH1Z_driverdoor_actionhijack_getin_section01_fb"
local door_anim_B = "All_AH1Z_driverdoor_actionhijack_getin_section02_fb"
local door_anim_C = "All_AH1Z_driverdoor_actionhijack_getin_section03_fb"
local door_anim_D = "All_AH1Z_driverdoor_actionhijack_getin_section04_fb"
local door_anim_E = "All_AH1Z_driverdoor_actionhijack_getin_section05_fb"
local door_anim_F = "All_AH1Z_driverdoor_actionhijack_getin_section06_fb"
local door_anim_AFail = "All_AH1Z_driverdoor_actionhijack_getin_section01failure_fb"
local door_anim_DLoopFail = "All_AH1Z_driverdoor_actionhijack_getin_section02loopfailure_fb"
local door_anim_CFail = "All_AH1Z_driverdoor_actionhijack_getin_section03failure_fb"
local door_anim_DLoopA = "all_AH1Z_driverdoor_actionhijack_getin_section02loopa_fb"
local door_anim_DLoopB = "all_AH1Z_driverdoor_actionhijack_getin_section02loopb_fb"
local door_anim_DLoopC = "all_AH1Z_driverdoor_actionhijack_getin_section02loopc_fb"
local door_anim_DLoopD = "all_AH1Z_driverdoor_actionhijack_getin_section02loopd_fb"
local door_anim_DLoopE = "all_AH1Z_driverdoor_actionhijack_getin_section02loopf_fb"
local door_anim_DLoopF = "all_AH1Z_driverdoor_actionhijack_getin_section02loopg_fb"
local door_anim_DLoopG = "all_AH1Z_driverdoor_actionhijack_getin_section02looph_fb"
local door_anim_DLoopH = "all_AH1Z_driverdoor_actionhijack_getin_section02loopi_fb"

function Init()
  Debug.Printf("Hijack_AH1Z: Loading vehicle animation assets")
  local assetType = "animation"
  Pg.LoadAsset(door_anim_A, assetType)
  Pg.LoadAsset(door_anim_B, assetType)
  Pg.LoadAsset(door_anim_C, assetType)
  Pg.LoadAsset(door_anim_D, assetType)
  Pg.LoadAsset(door_anim_E, assetType)
  Pg.LoadAsset(door_anim_F, assetType)
  Pg.LoadAsset(door_anim_AFail, assetType)
  Pg.LoadAsset(door_anim_CFail, assetType)
  Pg.LoadAsset(door_anim_DLoopFail, assetType)
  Pg.LoadAsset(door_anim_DLoopA, assetType)
  Pg.LoadAsset(door_anim_DLoopB, assetType)
  Pg.LoadAsset(door_anim_DLoopC, assetType)
  Pg.LoadAsset(door_anim_DLoopD, assetType)
  Pg.LoadAsset(door_anim_DLoopE, assetType)
  Pg.LoadAsset(door_anim_DLoopF, assetType)
  Pg.LoadAsset(door_anim_DLoopG, assetType)
  Pg.LoadAsset(door_anim_DLoopH, assetType)
end

function Deinit()
  Debug.Printf("Hijack_AH1Z: Unloading vehicle animation assets")
  local assetType = "animation"
  Pg.UnloadAsset(door_anim_A, assetType)
  Pg.UnloadAsset(door_anim_B, assetType)
  Pg.UnloadAsset(door_anim_C, assetType)
  Pg.UnloadAsset(door_anim_D, assetType)
  Pg.UnloadAsset(door_anim_E, assetType)
  Pg.UnloadAsset(door_anim_F, assetType)
  Pg.UnloadAsset(door_anim_AFail, assetType)
  Pg.UnloadAsset(door_anim_CFail, assetType)
  Pg.UnloadAsset(door_anim_DLoopFail, assetType)
  Pg.UnloadAsset(door_anim_DLoopA, assetType)
  Pg.UnloadAsset(door_anim_DLoopB, assetType)
  Pg.UnloadAsset(door_anim_DLoopC, assetType)
  Pg.UnloadAsset(door_anim_DLoopD, assetType)
  Pg.UnloadAsset(door_anim_DLoopE, assetType)
  Pg.UnloadAsset(door_anim_DLoopF, assetType)
  Pg.UnloadAsset(door_anim_DLoopG, assetType)
  Pg.UnloadAsset(door_anim_DLoopH, assetType)
end

function StartHijack(hijackerObject, hijackeeObject, seatObject, vehicleObject)
  local bResult = MrxActionHijack.CheckGoodStart(vehicleObject, MrxActionHijack.RULESET_HELICOPTER)
  Debug.Printf("StartHijack: CheckGoodStart: " .. tostring(bResult))
  if bResult then
    _THIS:Initialize(nil, hijackerObject, hijackeeObject, seatObject, vehicleObject)
    return true
  end
  return false
end

function Initialize(mModule, self, hijackerObject, hijackeeObject, seatObject, vehicleObject)
  self = self or {}
  self._hijacker = hijackerObject
  self._hijackee = hijackeeObject
  self._seat = seatObject
  self._vehicle = vehicleObject
  setmetatable(self, {__index = mModule})
  self._hijackerPlayer = Object.IsPlayerControlled(self._hijacker)
  MrxActionHijack.InitializeActionHijack(self)
  FaceState_anim_BLoopA = {
    hijacker = {
      faceStateA = {state = "angry", weight = 1}
    },
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
      faceStateA = {state = "Scared", weight = 0.25},
      faceStateB = {state = "EyesShut", weight = 1},
      faceStateC = {state = "angry", weight = 0}
    }
  }
  FaceState_anim_BLoopD = {
    hijackee = {
      faceStateA = {state = "Scared", weight = 0},
      faceStateB = {state = "EyesShut", weight = 0},
      faceStateC = {state = "angry", weight = 0.25}
    }
  }
  FaceState_anim_BLoopF = {
    hijackee = {
      faceStateA = {state = "angry", weight = 0.5},
      faceStateB = {state = "Scared", weight = 0}
    }
  }
  FaceState_anim_BLoopG = {
    hijackee = {
      faceStateA = {state = "angry", weight = 0.75}
    }
  }
  FaceState_anim_BLoopH = {
    hijackee = {
      faceStateA = {state = "angry", weight = 1}
    }
  }
  self[1] = {
    hijackerAnimation = "ActionHijackHijackerA",
    hijackeeAnimation = "ActionHijackHijackeeA",
    hijackerAnimationFail = "ActionHijackHijackerFailA",
    hijackeeAnimationFail = "ActionHijackHijackeeFailA",
    vehicleAnimation = door_anim_A,
    vehicleAnimationFail = door_anim_AFail,
    miniGameStartDelay = 1.13,
    miniGame = {
      bExtraHudParameters = true,
      nTranslucency = 255,
      nXPosition = 0,
      nYPosition = -0.45,
      bShowTimer = false,
      nTranslucency = 255,
      nTimeOut = 1,
      sAction = "press",
      button = Controller.LStick_Up,
      nScale = 1.2,
      nHudButtonMotionSpeed = 0.25
    },
    tMultiEvents = {
      {
        nTime = 0.13,
        tControllerRumble = {nlength = 0.2},
        tCameraShake = {fSetCameraAmplitude = 2.5, fSetCameraShake = 0.1},
        tCameraParams = {
          nStartNear = 0,
          nEndNear = 2,
          nStartFar = 6,
          nEndFar = 10
        }
      }
    },
    OnFailureAnimationBegin = OnFailureEvents,
    tFailureMultiEvents = {
      {
        nTime = 0.2,
        tControllerRumble = {nlength = 0.5},
        tCameraShake = {fSetCameraAmplitude = 2.5, fSetCameraShake = 0.1}
      }
    }
  }
  self[2] = {
    hijackerAnimation = "ActionHijackHijackerB",
    hijackeeAnimation = "ActionHijackHijackeeB",
    vehicleAnimation = door_anim_B,
    tMultiEvents = {
      {
        nTime = 0,
        tCameraParams = {
          nStartNear = 0,
          nEndNear = 1,
          nStartFar = 6,
          nEndFar = 8
        }
      }
    }
  }
  self[3] = {
    hijackerAnimationFail = "ActionHijackHijackerFailB",
    hijackeeAnimationFail = "ActionHijackHijackeeFailB",
    vehicleAnimationFail = door_anim_DLoopFail,
    tCharactersFaceStates = {
      hijackerFail = {
        faceStateA = {state = "angry", weight = 0},
        faceStateB = {state = "Scared", weight = 1}
      },
      hijackeeFail = {
        faceStateA = {state = "angry", weight = 1}
      }
    },
    nReactiveLoop = 2,
    tHijackerAnimations = {
      "ActionHijackHijackerDLoopA",
      "ActionHijackHijackerDLoopB",
      "ActionHijackHijackerDLoopC",
      "ActionHijackHijackerDLoopD",
      "ActionHijackHijackerDLoopE",
      "ActionHijackHijackerDLoopF",
      "ActionHijackHijackerDLoopG",
      "ActionHijackHijackerDLoopH"
    },
    tHijackeeAnimations = {
      "ActionHijackHijackeeBLoopA",
      "ActionHijackHijackeeBLoopB",
      "ActionHijackHijackeeBLoopC",
      "ActionHijackHijackeeBLoopD",
      "ActionHijackHijackeeBLoopE",
      "ActionHijackHijackeeBLoopF",
      "ActionHijackHijackeeBLoopG",
      "ActionHijackHijackerBLoopH"
    },
    tVehicleAnimations = {
      door_anim_DLoopA,
      door_anim_DLoopB,
      door_anim_DLoopC,
      door_anim_DLoopD,
      door_anim_DLoopE,
      door_anim_DLoopF,
      door_anim_DLoopG,
      door_anim_DLoopH
    },
    tReactiveLoopFaceStates = {
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
      bExtraHudParameters = true,
      nTranslucency = 255,
      nXPosition = 0,
      nYPosition = 0.8,
      bShowTimer = false,
      nTimeOut = 35,
      sAction = "tap",
      button = Controller.RPad_Down,
      nDriverDifficulty = 1,
      nSuccessThreshold = 1
    },
    OnFailureAnimationBegin = OnFailureEvents,
    tFailureMultiEvents = {
      {
        nTime = 0.5,
        tControllerRumble = {nlength = 0.5},
        tCameraShake = {fSetCameraAmplitude = 2.5, fSetCameraShake = 0.1}
      }
    }
  }
  self[4] = {
    hijackerAnimation = "ActionHijackHijackerC",
    hijackeeAnimation = "ActionHijackHijackeeC",
    vehicleAnimation = door_anim_C,
    hijackerAnimationFail = "ActionHijackHijackerFailC",
    hijackeeAnimationFail = "ActionHijackHijackeeFailC",
    vehicleAnimationFail = door_anim_CFail,
    miniGameStartDelay = 1.88,
    miniGame = {
      bExtraHudParameters = true,
      nTranslucency = 255,
      nXPosition = -0.5,
      nYPosition = 0.2,
      bShowTimer = false,
      nTimeOut = 0.75,
      sAction = "press",
      button = Controller.RPad_Left,
      nHudButtonMotionSpeed = 0.2
    },
    tMultiEvents = {
      {
        nTime = 0,
        tCameraParams = {
          nStartNear = 0,
          nEndNear = 2,
          nStartFar = 6,
          nEndFar = 10
        }
      },
      {
        nTime = 1.2,
        tControllerRumble = {nlength = 0.2},
        tCameraShake = {fSetCameraAmplitude = 2.5, fSetCameraShake = 0.1}
      }
    },
    OnFailureAnimationBegin = OnFailureEvents,
    tFailureMultiEvents = {
      {
        nTime = 0.67,
        tControllerRumble = {nlength = 0.5}
      }
    }
  }
  self[5] = {
    hijackerAnimation = "ActionHijackHijackerD",
    hijackeeAnimation = "ActionHijackHijackeeD",
    vehicleAnimation = door_anim_D,
    tMultiEvents = {
      {
        nTime = 0.06,
        tControllerRumble = {nlength = 0.2},
        tCameraShake = {fSetCameraAmplitude = 2.5, fSetCameraShake = 0.1},
        tCameraParams = {
          nStartNear = 0,
          nEndNear = 2,
          nStartFar = 6,
          nEndFar = 10
        }
      }
    }
  }
  self[6] = {
    hijackerAnimation = "ActionHijackHijackerE",
    hijackeeAnimation = "ActionHijackHijackeeE",
    vehicleAnimation = door_anim_E,
    hijackerAnimationFail = "ActionHijackHijackerFailC",
    hijackeeAnimationFail = "ActionHijackHijackeeFailC",
    vehicleAnimationFail = door_anim_CFail,
    miniGameStartDelay = 1.4,
    miniGame = {
      bExtraHudParameters = true,
      nTranslucency = 255,
      nXPosition = 0.5,
      nYPosition = 0.2,
      bShowTimer = false,
      nTimeOut = 0.5,
      sAction = "press",
      button = Controller.RPad_Right,
      nHudButtonMotionSpeed = 0.2
    },
    tMultiEvents = {
      {
        nTime = 0,
        tCameraParams = {
          nStartNear = 0,
          nEndNear = 2,
          nStartFar = 6,
          nEndFar = 10
        }
      },
      {
        nTime = 0.1,
        tControllerRumble = {nlength = 0.2},
        tCameraShake = {fSetCameraAmplitude = 2.5, fSetCameraShake = 0.1}
      }
    },
    OnFailureAnimationBegin = OnFailureEvents,
    tFailureMultiEvents = {
      {
        nTime = 0.67,
        tControllerRumble = {nlength = 0.5}
      }
    }
  }
  self[7] = {
    hijackerAnimation = "ActionHijackHijackerF",
    hijackeeAnimation = "ActionHijackHijackeeF",
    vehicleAnimation = door_anim_F,
    tMultiEvents = {
      {
        tCameraParams = {
          nStartNear = 0,
          nEndNear = 2,
          nStartFar = 6,
          nEndFar = 10
        }
      },
      {
        nTime = 0.1,
        tControllerRumble = {nlength = 0.2},
        tCameraShake = {fSetCameraAmplitude = 2.5, fSetCameraShake = 0.1}
      }
    },
    bDriverDoneDead = true
  }
  MrxActionHijack.Begin(self, 1)
  return self
end

function OnFailureEvents(self, nCurrent)
  Debug.Printf("OnFailureAnimationBegin: nCurrent: " .. tostring(nCurrent))
  local tFailEvents = self[self.nCurrent].tFailureMultiEvents
  local oLocalPlayer = self._hijacker
  MrxActionHijack._ProcessMultiEventTable(oLocalPlayer, tFailEvents)
end
