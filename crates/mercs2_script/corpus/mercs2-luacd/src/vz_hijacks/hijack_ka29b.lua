import("MrxActionHijack")
local door_anim_A = "All_KA29B_driverdoor_actionhijack_getin_section01_fb"
local door_anim_B = "All_KA29B_driverdoor_actionhijack_getin_section02_fb"
local door_anim_C = "All_KA29B_driverdoor_actionhijack_getin_section03_fb"
local door_anim_BLoop = "All_KA29B_driverdoor_actionhijack_getin_section01loopb_fb"
local door_anim_AFail = "All_KA29B_driverdoor_actionhijack_getin_section01failure_fb"

function Init()
  Debug.Printf("Hijack_KA29B: Loading vehicle animation assets")
  local assetType = "animation"
  Pg.LoadAsset(door_anim_A, assetType)
  Pg.LoadAsset(door_anim_B, assetType)
  Pg.LoadAsset(door_anim_C, assetType)
  Pg.LoadAsset(door_anim_BLoop, assetType)
  Pg.LoadAsset(door_anim_AFail, assetType)
end

function Deinit()
  Debug.Printf("Hijack_KA29B: Unloading vehicle animation assets")
  local assetType = "animation"
  Pg.UnloadAsset(door_anim_A, assetType)
  Pg.UnloadAsset(door_anim_B, assetType)
  Pg.UnloadAsset(door_anim_C, assetType)
  Pg.UnloadAsset(door_anim_BLoop, assetType)
  Pg.UnloadAsset(door_anim_AFail, assetType)
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
    miniGameStartDelay = 2.41,
    miniGame = {
      nTimeOut = 0.75,
      bExtraHudParameters = true,
      nTranslucency = 255,
      nXPosition = 0,
      nYPosition = 0.8,
      bShowTimer = false,
      sAction = "press",
      button = Controller.LStick_Down,
      nScale = 1.2,
      nHudButtonMotionSpeed = 0.25
    },
    tMultiEvents = {
      {
        nTime = 0.5,
        tControllerRumble = {nlength = 0.2},
        tCameraShake = {fSetCameraAmplitude = 2.5, fSetCameraShake = 0.1}
      },
      {
        nTime = 1.6333,
        tControllerRumble = {nlength = 0.02}
      }
    },
    OnFailureAnimationBegin = OnFailureEvents,
    tFailureMultiEvents = {
      {
        nTime = 0.7,
        tControllerRumble = {nlength = 0.2},
        tCameraShake = {fSetCameraAmplitude = 2.5, fSetCameraShake = 0.1}
      },
      {
        nTime = 2.66,
        tControllerRumble = {nlength = 0.5},
        tCameraShake = {fSetCameraAmplitude = 2.5, fSetCameraShake = 0.1}
      }
    }
  }
  self[2] = {
    hijackerAnimation = "ActionHijackHijackerB",
    hijackeeAnimation = "ActionHijackHijackeeB",
    vehicleAnimation = door_anim_B,
    hijackerAnimationFail = "ActionHijackHijackerFailA",
    hijackeeAnimationFail = "ActionHijackHijackeeFailA",
    vehicleAnimationFail = door_anim_AFail,
    miniGameStartDelay = 1.46,
    miniGame = {
      nTimeOut = 1,
      bExtraHudParameters = true,
      nTranslucency = 255,
      nXPosition = 0.5,
      nYPosition = 0.2,
      bShowTimer = false,
      sAction = "press",
      button = Controller.RPad_Right,
      nHudButtonMotionSpeed = 0.2
    },
    tMultiEvents = {
      {
        nTime = 1.9333,
        tControllerRumble = {nlength = 0.2},
        tCameraShake = {fSetCameraAmplitude = 2, fSetCameraShake = 0.1}
      }
    },
    OnFailureAnimationBegin = OnFailureEvents,
    tFailureMultiEvents = {
      {
        nTime = 0.7,
        tControllerRumble = {nlength = 0.2},
        tCameraShake = {fSetCameraAmplitude = 2.5, fSetCameraShake = 0.1}
      },
      {
        nTime = 2.66,
        tControllerRumble = {nlength = 0.5},
        tCameraShake = {fSetCameraAmplitude = 2.5, fSetCameraShake = 0.1}
      }
    }
  }
  self[3] = {
    hijackerAnimationFail = "ActionHijackHijackerFailA",
    hijackeeAnimationFail = "ActionHijackHijackeeFailA",
    vehicleAnimationFail = door_anim_AFail,
    nReactiveLoop = 3,
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
      "ActionHijackHijackeeBLoopH"
    },
    tVehicleAnimations = {
      door_anim_BLoop,
      door_anim_BLoop,
      door_anim_BLoop,
      door_anim_BLoop,
      door_anim_BLoop,
      door_anim_BLoop,
      door_anim_BLoop,
      door_anim_BLoop
    },
    miniGameStartDelay = 0.6,
    miniGame = {
      nTimeOut = 20,
      bExtraHudParameters = true,
      nTranslucency = 255,
      nXPosition = -0.5,
      nYPosition = 0.2,
      bShowTimer = false,
      sAction = "tap",
      button = Controller.RPad_Left,
      nDriverDifficulty = 0.8,
      nSuccessThreshold = 1
    },
    tFailureMultiEvents = {
      {
        nTime = 0.7,
        tControllerRumble = {nlength = 0.2},
        tCameraShake = {fSetCameraAmplitude = 2.5, fSetCameraShake = 0.1}
      },
      {
        nTime = 2.66,
        tControllerRumble = {nlength = 0.5},
        tCameraShake = {fSetCameraAmplitude = 2.5, fSetCameraShake = 0.1}
      }
    }
  }
  self[4] = {
    hijackerAnimation = "ActionHijackHijackerC",
    hijackeeAnimation = "ActionHijackHijackeeC",
    vehicleAnimation = door_anim_C,
    tMultiEvents = {
      {
        nTime = 0.46,
        tControllerRumble = {nlength = 0.2},
        tCameraShake = {fSetCameraAmplitude = 2, fSetCameraShake = 0.1}
      },
      {
        nTime = 1.2,
        tControllerRumble = {nlength = 0.2},
        tCameraShake = {fSetCameraAmplitude = 2, fSetCameraShake = 0.1}
      },
      {
        nTime = 2.06,
        tControllerRumble = {nlength = 0.2},
        tCameraShake = {fSetCameraAmplitude = 2.5, fSetCameraShake = 0.1}
      }
    },
    bDriverDoneRemove = true
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
