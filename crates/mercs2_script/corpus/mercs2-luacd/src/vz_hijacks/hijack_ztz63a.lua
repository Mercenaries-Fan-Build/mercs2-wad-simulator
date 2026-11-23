import("MrxActionHijack")
local door_anim_A = "all_ZTZ63a_driverhatch_actionhijack_getin_section01_fb"
local door_anim_AFail = "all_ZTZ63a_driverhatch_actionhijack_getin_section01fail_fb"
local door_anim_B = "all_ZTZ63a_driverhatch_actionhijack_getin_section02_fb"
local door_anim_DLoopA = "all_ZTZ63a_driverhatch_actionhijack_getin_loopa_fb"
local door_anim_DLoopFail = "all_ZTZ63a_driverhatch_actionhijack_getin_section02loopfailure_fb"
local door_anim_C = "all_ZTZ63a_driverhatch_actionhijack_getin_section03_fb"

function Init()
  Debug.Printf("Hijack_ZTZ63a: Loading vehicle animation assets")
  local assetType = "animation"
  Pg.LoadAsset(door_anim_A, assetType)
  Pg.LoadAsset(door_anim_AFail, assetType)
  Pg.LoadAsset(door_anim_B, assetType)
  Pg.LoadAsset(door_anim_DLoopA, assetType)
  Pg.LoadAsset(door_anim_DLoopFail, assetType)
  Pg.LoadAsset(door_anim_C, assetType)
end

function Deinit()
  Debug.Printf("Hijack_ZTZ63a: Unloading vehicle animation assets")
  local assetType = "animation"
  Pg.UnloadAsset(door_anim_A, assetType)
  Pg.UnloadAsset(door_anim_AFail, assetType)
  Pg.UnloadAsset(door_anim_B, assetType)
  Pg.UnloadAsset(door_anim_DLoopA, assetType)
  Pg.UnloadAsset(door_anim_DLoopFail, assetType)
  Pg.UnloadAsset(door_anim_C, assetType)
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
  MrxActionHijack.InitializeActionHijack(self)
  MrxActionHijack.TankPrep(self)
  self[1] = {
    hijackerAnimation = "ActionHijackHijackerA",
    hijackeeAnimation = "ActionHijackHijackeeA",
    hijackerAnimationFail = "ActionHijackHijackerFailA",
    hijackeeAnimationFail = "ActionHijackHijackeeFailA",
    vehicleAnimation = door_anim_A,
    vehicleAnimationFail = door_anim_AFail,
    tMultiEvents = {
      {
        nTime = 0.1,
        tControllerRumble = {nlength = 0.2},
        tCameraShake = {fSetCameraAmplitude = 2, fSetCameraShake = 0.1}
      }
    },
    miniGameStartDelay = 1.71,
    miniGame = {
      nTimeOut = 0.75,
      sAction = "press",
      bExtraHudParameters = true,
      nTranslucency = 255,
      bShowTimer = false,
      nHudButtonMotionSpeed = 0.25,
      nXPosition = 0.5,
      nYPosition = 0.2,
      button = Controller.LStick_Right,
      nScale = 1.2
    },
    OnFailureAnimationBegin = OnFailureEvents,
    tFailureMultiEvents = {
      {
        nTime = 0.46,
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
        nTime = 0.43,
        tControllerRumble = {nlength = 0.2},
        tCameraShake = {fSetCameraAmplitude = 2, fSetCameraShake = 0.1}
      },
      {
        nTime = 3.5,
        tControllerRumble = {nlength = 0.2},
        tCameraShake = {fSetCameraAmplitude = 1, fSetCameraShake = 0.1}
      },
      {
        nTime = 4.23,
        tControllerRumble = {nlength = 0.2},
        tCameraShake = {fSetCameraAmplitude = 1, fSetCameraShake = 0.1}
      }
    }
  }
  self[3] = {
    hijackerAnimationFail = "ActionHijackHijackerFailB",
    hijackeeAnimationFail = "ActionHijackHijackeeFailB",
    vehicleAnimationFail = door_anim_DLoopFail,
    nReactiveLoop = 2,
    tHijackerAnimations = {
      "ActionHijackHijackerDLoopH",
      "ActionHijackHijackerDLoopG",
      "ActionHijackHijackerDLoopF",
      "ActionHijackHijackerDLoopD",
      "ActionHijackHijackerDLoopC",
      "ActionHijackHijackerDLoopB",
      "ActionHijackHijackerDLoopA"
    },
    tHijackeeAnimations = {
      "ActionHijackHijackeeBLoopH",
      "ActionHijackHijackeeBLoopG",
      "ActionHijackHijackeeBLoopF",
      "ActionHijackHijackeeBLoopD",
      "ActionHijackHijackeeBLoopC",
      "ActionHijackHijackeeBLoopB",
      "ActionHijackHijackeeBLoopA"
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
      nTimeOut = 20,
      sAction = "tap",
      button = Controller.RPad_Down,
      bExtraHudParameters = true,
      nTranslucency = 255,
      nXPosition = 0,
      nYPosition = 0.8,
      bShowTimer = false,
      nDriverDifficulty = 0.7,
      nSuccessThreshold = 1
    },
    OnFailureAnimationBegin = OnFailureEvents,
    tFailureMultiEvents = {
      {
        nTime = 0.2,
        tControllerRumble = {nlength = 0.5},
        tCameraShake = {fSetCameraAmplitude = 1, fSetCameraShake = 0.1}
      }
    }
  }
  self[4] = {
    hijackerAnimation = "ActionHijackHijackerC",
    hijackeeAnimation = "ActionHijackHijackeeC",
    vehicleAnimation = door_anim_C,
    tMultiEvents = {
      {
        nTime = 0,
        tControllerRumble = {nlength = 0.2}
      },
      {
        nTime = 1.03,
        tControllerRumble = {nlength = 0.2},
        tCameraShake = {fSetCameraAmplitude = 2, fSetCameraShake = 0.1}
      },
      {
        nTime = 1.73,
        tControllerRumble = {nlength = 0.2},
        tCameraShake = {fSetCameraAmplitude = 2, fSetCameraShake = 0.1}
      },
      {
        nTime = 2.8,
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
