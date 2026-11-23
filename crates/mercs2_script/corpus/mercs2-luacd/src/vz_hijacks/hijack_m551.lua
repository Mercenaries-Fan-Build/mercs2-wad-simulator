import("MrxActionHijack")
local door_anim_A = "all_M551_driverhatch_actionhijack_getin_section01_fb"
local door_anim_B = "all_M551_driverhatch_actionhijack_getin_section02_fb"
local door_anim_ALoopA = "all_M551_driverhatch_actionhijack_getin_section01loopa_fb"
local door_anim_ALoopFail = "all_M551_driverhatch_actionhijack_getin_section01loopfailure_fb"

function Init()
  Debug.Printf("Hijack_M551: Loading vehicle animation assets")
  local assetType = "animation"
  Pg.LoadAsset(door_anim_A, assetType)
  Pg.LoadAsset(door_anim_B, assetType)
  Pg.LoadAsset(door_anim_ALoopA, assetType)
  Pg.LoadAsset(door_anim_ALoopFail, assetType)
end

function Deinit()
  Debug.Printf("Hijack_M551: Unloading vehicle animation assets")
  local assetType = "animation"
  Pg.UnloadAsset(door_anim_A, assetType)
  Pg.UnloadAsset(door_anim_B, assetType)
  Pg.UnloadAsset(door_anim_ALoopA, assetType)
  Pg.UnloadAsset(door_anim_ALoopFail, assetType)
end

function StartHijack(hijackerObject, hijackeeObject, seatObject, vehicleObject)
  _THIS:Initialize(nil, hijackerObject, hijackeeObject, seatObject, vehicleObject)
  return true
end

function Initialize(mModule, self, hijackerObject, hijackeeObject, seatObject, vehicleObject)
  self = self or {}
  self._hijacker = hijackerObject
  self._hijackee = hijackeeObject
  self._seat = seatObject
  self._vehicle = vehicleObject
  setmetatable(self, {__index = mModule})
  self._hijackerPlayer = Object.IsPlayerControlled(self._hijacker)
  self._OnActionHijackComplete = OnActionHijackComplete
  MrxActionHijack.InitializeActionHijack(self)
  MrxActionHijack.TankPrep(self)
  FaceState_anim_BLoopA = {
    hijacker = {
      faceStateA = {state = "angry", weight = 1}
    },
    hijackee = {
      faceStateA = {state = "Scared", weight = 1}
    }
  }
  FaceState_anim_BLoopB = {
    hijacker = {
      faceStateA = {state = "angry", weight = 0.75}
    },
    hijackee = {
      faceStateA = {state = "Scared", weight = 0.75}
    }
  }
  FaceState_anim_BLoopC = {
    hijackee = {
      faceStateA = {state = "Scared", weight = 0.5},
      faceStateB = {state = "EyesShut", weight = 1},
      faceStateC = {state = "angry", weight = 0}
    }
  }
  FaceState_anim_BLoopD = {
    hijackee = {
      faceStateA = {state = "Scared", weight = 0.25},
      faceStateB = {state = "EyesShut", weight = 0},
      faceStateC = {state = "angry", weight = 0}
    }
  }
  FaceState_anim_BLoopE = {
    hijackee = {
      faceStateA = {state = "angry", weight = 0.25},
      faceStateB = {state = "Scared", weight = 0}
    }
  }
  FaceState_anim_BLoopF = {
    hijackee = {
      faceStateA = {state = "angry", weight = 0.5}
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
    tCharactersFaceStates = {
      hijacker = {
        faceStateA = {state = "angry", weight = 0.5}
      }
    },
    miniGameStartDelay = 1,
    miniGame = {
      nTimeOut = 1,
      bExtraHudParameters = true,
      nTranslucency = 255,
      nXPosition = -0.5,
      nYPosition = 0.2,
      nScale = 1.2,
      bShowTimer = false,
      sAction = "press",
      button = Controller.LStick_Left,
      nHudButtonMotionSpeed = 0.25
    },
    OnFailureAnimationBegin = OnFailureEvents,
    tFailureMultiEvents = {
      {
        nTime = 0.6,
        tControllerRumble = {nlength = 0.5},
        tCameraShake = {fSetCameraAmplitude = 1, fSetCameraShake = 0.1}
      }
    }
  }
  self[2] = {
    hijackerAnimationFail = "ActionHijackHijackerFailA",
    hijackeeAnimationFail = "ActionHijackHijackeeFailA",
    vehicleAnimationFail = door_anim_ALoopFail,
    nReactiveLoop = 3,
    tHijackerAnimations = {
      "ActionHijackHijackerDLoopH",
      "ActionHijackHijackerDLoopG",
      "ActionHijackHijackerDLoopF",
      "ActionHijackHijackerDLoopE",
      "ActionHijackHijackerDLoopD",
      "ActionHijackHijackerDLoopC",
      "ActionHijackHijackerDLoopB",
      "ActionHijackHijackerDLoopA"
    },
    tHijackeeAnimations = {
      "ActionHijackHijackeeBLoopH",
      "ActionHijackHijackeeBLoopG",
      "ActionHijackHijackeeBLoopF",
      "ActionHijackHijackeeBLoopE",
      "ActionHijackHijackeeBLoopD",
      "ActionHijackHijackeeBLoopC",
      "ActionHijackHijackeeBLoopB",
      "ActionHijackHijackeeBLoopA"
    },
    tVehicleAnimations = {
      door_anim_ALoopA,
      door_anim_ALoopA,
      door_anim_ALoopA,
      door_anim_ALoopA,
      door_anim_ALoopA,
      door_anim_ALoopA,
      door_anim_ALoopA,
      door_anim_ALoopA
    },
    tReactiveLoopFaceStates = {
      FaceState_anim_BLoopH,
      FaceState_anim_BLoopG,
      FaceState_anim_BLoopF,
      FaceState_anim_BLoopE,
      FaceState_anim_BLoopD,
      FaceState_anim_BLoopC,
      FaceState_anim_BLoopB,
      FaceState_anim_BLoopA
    },
    miniGameStartDelay = 0.1,
    miniGame = {
      nTimeOut = 35,
      bExtraHudParameters = true,
      nTranslucency = 255,
      nXPosition = 0,
      nYPosition = -0.45,
      bShowTimer = false,
      sAction = "tap",
      button = Controller.RPad_Up,
      nDriverDifficulty = 1.2,
      nSuccessThreshold = 1
    },
    OnFailureAnimationBegin = OnFailureEvents,
    tFailureMultiEvents = {
      {
        nTime = 0.6,
        tControllerRumble = {nlength = 0.5},
        tCameraShake = {fSetCameraAmplitude = 1, fSetCameraShake = 0.1}
      }
    }
  }
  self[3] = {
    hijackerAnimation = "ActionHijackHijackerB",
    hijackeeAnimation = "ActionHijackHijackeeB",
    vehicleAnimation = door_anim_B,
    tMultiEvents = {
      {
        nTime = 0.56,
        tControllerRumble = {nlength = 0.2},
        tCameraShake = {fSetCameraAmplitude = 1, fSetCameraShake = 0.1}
      },
      {
        nTime = 1.1,
        tControllerRumble = {nlength = 0.2},
        tCameraShake = {fSetCameraAmplitude = 2, fSetCameraShake = 0.1}
      },
      {
        nTime = 2.2,
        tControllerRumble = {nlength = 0.5},
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
