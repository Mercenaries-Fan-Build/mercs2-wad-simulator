import("MrxActionHijack")
local door_anim_A = "All_Mi26_driverdoor_actionhijack_getin_section01_fb"
local door_anim_B = "All_Mi26_driverdoor_actionhijack_getin_section02_fb"
local door_anim_AFail = "All_Mi26_driverdoor_actionhijack_getin_section01failure_fb"

function Init()
  Debug.Printf("Hijack_Mi26: Loading vehicle animation assets")
  local assetType = "animation"
  Pg.LoadAsset(door_anim_A, assetType)
  Pg.LoadAsset(door_anim_B, assetType)
  Pg.LoadAsset(door_anim_AFail, assetType)
end

function Deinit()
  Debug.Printf("Hijack_Mi26: Unloading vehicle animation assets")
  local assetType = "animation"
  Pg.UnloadAsset(door_anim_A, assetType)
  Pg.UnloadAsset(door_anim_B, assetType)
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
    vehicleAnimation = door_anim_A,
    hijackerAnimationFail = "ActionHijackHijackerFailA",
    hijackeeAnimationFail = "ActionHijackHijackeeFailA",
    vehicleAnimationFail = door_anim_AFail,
    miniGameStartDelay = 5.16,
    miniGame = {
      nTimeOut = 2.5,
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
        nTime = 0.13,
        tControllerRumble = {nlength = 0.2},
        tCameraShake = {fSetCameraAmplitude = 2.5, fSetCameraShake = 0.1}
      },
      {
        nTime = 2.06,
        tControllerRumble = {nlength = 0.2}
      },
      {
        nTime = 7.13,
        tControllerRumble = {nlength = 0.2}
      }
    },
    OnFailureAnimationBegin = OnFailureEvents,
    tFailureMultiEvents = {
      {
        nTime = 0.66,
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
    hijackerAnimationFail = "ActionHijackHijackerFailA",
    hijackeeAnimationFail = "ActionHijackHijackeeFailA",
    vehicleAnimationFail = door_anim_AFail,
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
      "ActionHijackHijackeeBLoopH"
    },
    tVehicleAnimations = {
      door_anim_B,
      door_anim_B,
      door_anim_B,
      door_anim_B,
      door_anim_B,
      door_anim_B,
      door_anim_B,
      door_anim_B
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
        nTime = 0.66,
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
    hijackerAnimation = "ActionHijackHijackerB",
    hijackeeAnimation = "ActionHijackHijackeeB",
    vehicleAnimation = door_anim_B,
    tMultiEvents = {
      {
        nTime = 0.43,
        tControllerRumble = {nlength = 0.2},
        tCameraShake = {fSetCameraAmplitude = 2, fSetCameraShake = 0.1}
      },
      {
        nTime = 1.1,
        tControllerRumble = {nlength = 0.2},
        tCameraShake = {fSetCameraAmplitude = 2, fSetCameraShake = 0.1}
      },
      {
        nTime = 1.96,
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
