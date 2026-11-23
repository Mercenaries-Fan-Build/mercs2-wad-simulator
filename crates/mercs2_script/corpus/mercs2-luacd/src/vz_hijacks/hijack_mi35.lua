import("MrxActionHijack")
local door_anim_A = "All_Mi35_driverdoor_actionhijack_getin_section01_fb"
local door_anim_B = "All_Mi35_driverdoor_actionhijack_getin_section02_fb"
local door_anim_C = "All_Mi35_driverdoor_actionhijack_getin_section03_fb"
local door_anim_D = "All_Mi35_driverdoor_actionhijack_getin_section04_fb"
local door_anim_E = "All_Mi35_driverdoor_actionhijack_getin_section05_fb"
local door_anim_F = "All_Mi35_driverdoor_actionhijack_getin_section06_fb"
local door_anim_AFail = "All_Mi35_driverdoor_actionhijack_getin_section01failure_fb"
local door_anim_DLoopFail = "All_Mi35_driverdoor_actionhijack_getin_section02loopfailure_fb"
local door_anim_CFail = "All_Mi35_driverdoor_actionhijack_getin_section03failure_fb"
local door_anim_DLoopA = "all_Mi35_driverdoor_actionhijack_getin_section02loopa_fb"

function Init()
  Debug.Printf("Hijack_Mi35: Loading vehicle animation assets")
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
end

function Deinit()
  Debug.Printf("Hijack_Mi35: Unloading vehicle animation assets")
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
  self[1] = {
    hijackerAnimation = "ActionHijackHijackerA",
    hijackeeAnimation = "ActionHijackHijackeeA",
    hijackerAnimationFail = "ActionHijackHijackerFailA",
    hijackeeAnimationFail = "ActionHijackHijackeeFailA",
    vehicleAnimation = door_anim_A,
    vehicleAnimationFail = door_anim_AFail,
    miniGameStartDelay = 0.96,
    miniGame = {
      bExtraHudParameters = true,
      nTranslucency = 255,
      nXPosition = -0.5,
      nYPosition = 0.2,
      bShowTimer = false,
      nTranslucency = 255,
      nTimeOut = 1,
      sAction = "press",
      button = Controller.LStick_Left,
      nScale = 1.2,
      nHudButtonMotionSpeed = 0.25
    },
    tMultiEvents = {
      {
        nTime = 0.16,
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
      door_anim_DLoopA,
      door_anim_DLoopA,
      door_anim_DLoopA,
      door_anim_DLoopA,
      door_anim_DLoopA,
      door_anim_DLoopA,
      door_anim_DLoopA
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
    miniGameStartDelay = 2.46,
    miniGame = {
      bExtraHudParameters = true,
      nTranslucency = 255,
      nXPosition = -0.5,
      nYPosition = 0.2,
      bShowTimer = false,
      nTimeOut = 0.5,
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
        nTime = 1.1,
        tControllerRumble = {nlength = 0.2},
        tCameraShake = {fSetCameraAmplitude = 2.5, fSetCameraShake = 0.1}
      }
    },
    OnFailureAnimationBegin = OnFailureEvents,
    tFailureMultiEvents = {
      {
        nTime = 0.66,
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
        nTime = 0.16,
        tControllerRumble = {nlength = 0.2},
        tCameraShake = {fSetCameraAmplitude = 2.5, fSetCameraShake = 0.1}
      }
    },
    OnFailureAnimationBegin = OnFailureEvents,
    tFailureMultiEvents = {
      {
        nTime = 0.66,
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
        nTime = 0.3,
        tControllerRumble = {nlength = 0.2},
        tCameraShake = {fSetCameraAmplitude = 2.5, fSetCameraShake = 0.1}
      },
      {
        nTime = 2.96,
        tControllerRumble = {nlength = 0.2}
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
