import("MrxActionHijack")
local door_anim_A = "All_Huey_driverdoor_actionhijack_section01_fb"
local door_anim_AFail = "All_Huey_driverdoor_actionhijack_fail01_fb"
local door_anim_B = "All_Huey_driverdoor_actionhijack_section02_fb"
local door_anim_BFail = "All_Huey_driverdoor_actionhijack_fail02_fb"
local door_anim_DLoopA = "All_Huey_driverdoor_actionhijack_getin_loopa_fb"
local door_anim_DLoopB = "All_Huey_driverdoor_actionhijack_getin_loopb_fb"
local door_anim_DLoopC = "All_Huey_driverdoor_actionhijack_getin_loopc_fb"
local door_anim_DLoopD = "All_Huey_driverdoor_actionhijack_getin_loopd_fb"
local door_anim_DLoopE = "All_Huey_driverdoor_actionhijack_getin_loope_fb"
local door_anim_DLoopF = "All_Huey_driverdoor_actionhijack_getin_loopf_fb"
local door_anim_DLoopG = "All_Huey_driverdoor_actionhijack_getin_loopg_fb"
local door_anim_DLoopH = "All_Huey_driverdoor_actionhijack_getin_looph_fb"
local door_anim_C = "All_Huey_driverdoor_actionhijack_section03_fb"

function Init()
  Debug.Printf("Hijack_Huey: Loading vehicle animation assets")
  local assetType = "animation"
  Pg.LoadAsset(door_anim_A, assetType)
  Pg.LoadAsset(door_anim_AFail, assetType)
  Pg.LoadAsset(door_anim_B, assetType)
  Pg.LoadAsset(door_anim_BFail, assetType)
  Pg.LoadAsset(door_anim_DLoopA, assetType)
  Pg.LoadAsset(door_anim_DLoopB, assetType)
  Pg.LoadAsset(door_anim_DLoopC, assetType)
  Pg.LoadAsset(door_anim_DLoopD, assetType)
  Pg.LoadAsset(door_anim_DLoopE, assetType)
  Pg.LoadAsset(door_anim_DLoopF, assetType)
  Pg.LoadAsset(door_anim_DLoopG, assetType)
  Pg.LoadAsset(door_anim_DLoopH, assetType)
  Pg.LoadAsset(door_anim_C, assetType)
end

function Deinit()
  Debug.Printf("Hijack_Huey: Unloading vehicle animation assets")
  local assetType = "animation"
  Pg.UnloadAsset(door_anim_A, assetType)
  Pg.UnloadAsset(door_anim_AFail, assetType)
  Pg.UnloadAsset(door_anim_B, assetType)
  Pg.UnloadAsset(door_anim_BFail, assetType)
  Pg.UnloadAsset(door_anim_DLoopA, assetType)
  Pg.UnloadAsset(door_anim_DLoopB, assetType)
  Pg.UnloadAsset(door_anim_DLoopC, assetType)
  Pg.UnloadAsset(door_anim_DLoopD, assetType)
  Pg.UnloadAsset(door_anim_DLoopE, assetType)
  Pg.UnloadAsset(door_anim_DLoopF, assetType)
  Pg.UnloadAsset(door_anim_DLoopG, assetType)
  Pg.UnloadAsset(door_anim_DLoopH, assetType)
  Pg.UnloadAsset(door_anim_C, assetType)
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
    miniGameStartDelay = 2.73,
    miniGame = {
      nTimeOut = 1,
      sAction = "press",
      bExtraHudParameters = true,
      nTranslucency = 255,
      nXPosition = 0,
      nYPosition = 0.7,
      bShowTimer = false,
      nHudButtonMotionSpeed = 0.25,
      button = Controller.LStick_Down,
      nScale = 1.2
    },
    tMultiEvents = {
      {
        nTime = 0.16,
        tControllerRumble = {nlength = 0.2},
        tCameraShake = {fSetCameraAmplitude = 2.5, fSetCameraShake = 0.1}
      },
      {
        nTime = 3.3,
        tControllerRumble = {nlength = 0.2}
      }
    },
    OnFailureAnimationBegin = OnFailureEvents,
    tFailureMultiEvents = {
      {
        nTime = 0.83,
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
        nTime = 0.5,
        tControllerRumble = {nlength = 0.2}
      },
      {
        nTime = 1.53,
        tControllerRumble = {nlength = 0.2},
        tCameraShake = {fSetCameraAmplitude = 2.5, fSetCameraShake = 0.1}
      }
    }
  }
  self[3] = {
    hijackerAnimationFail = "ActionHijackHijackerFailB",
    hijackeeAnimationFail = "ActionHijackHijackeeFailB",
    vehicleAnimationFail = door_anim_BFail,
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
      door_anim_DLoopA,
      door_anim_DLoopB,
      door_anim_DLoopC,
      door_anim_DLoopD,
      door_anim_DLoopE,
      door_anim_DLoopF,
      door_anim_DLoopG,
      door_anim_DLoopH
    },
    miniGameStartDelay = 0.01,
    miniGame = {
      nTimeOut = 35,
      sAction = "tap",
      button = Controller.RPad_Up,
      bExtraHudParameters = true,
      nTranslucency = 255,
      nXPosition = 0,
      nYPosition = -0.45,
      bShowTimer = false,
      nDriverDifficulty = 1.2,
      nSuccessThreshold = 1
    },
    OnFailureAnimationBegin = OnFailureEvents,
    tFailureMultiEvents = {
      {
        nTime = 0.1,
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
        nTime = 0.3,
        tControllerRumble = {nlength = 0.2},
        tCameraShake = {fSetCameraAmplitude = 2, fSetCameraShake = 0.1}
      },
      {
        nTime = 0.93,
        tControllerRumble = {nlength = 0.2},
        tCameraShake = {fSetCameraAmplitude = 2, fSetCameraShake = 0.1}
      },
      {
        nTime = 1.63,
        tControllerRumble = {nlength = 0.2},
        tCameraShake = {fSetCameraAmplitude = 2.5, fSetCameraShake = 0.1}
      },
      {
        nTime = 2.6,
        tControllerRumble = {nlength = 0.2}
      }
    },
    bDriverDoneDead = true
  }
  MrxActionHijack.Begin(self, 1)
  return self
end
