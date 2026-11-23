import("MrxActionHijack")
local door_anim_A = "All_PLZ45_driverdoor_actionhijack_getin_section01_fb"
local door_anim_AFail = "All_PLZ45_driverdoor_actionhijack_getin_failsection01_fb"
local door_anim_B = "All_PLZ45_driverdoor_actionhijack_getin_section02_fb"
local door_anim_BFail = "All_PLZ45_driverdoor_actionhijack_getin_failsection02_fb"
local door_anim_C = "All_PLZ45_driverdoor_actionhijack_getin_section03_fb"

function Init()
  Debug.Printf("Hijack_PLZ45: Loading vehicle animation assets")
  local assetType = "animation"
  Pg.LoadAsset(door_anim_A, assetType)
  Pg.LoadAsset(door_anim_AFail, assetType)
  Pg.LoadAsset(door_anim_B, assetType)
  Pg.LoadAsset(door_anim_BFail, assetType)
  Pg.LoadAsset(door_anim_C, assetType)
end

function Deinit()
  Debug.Printf("Hijack_PLZ45: Unloading vehicle animation assets")
  local assetType = "animation"
  Pg.UnloadAsset(door_anim_A, assetType)
  Pg.UnloadAsset(door_anim_AFail, assetType)
  Pg.UnloadAsset(door_anim_B, assetType)
  Pg.UnloadAsset(door_anim_BFail, assetType)
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
  local bResult = Vehicle.SetTurretYaw(self._vehicle, "main_turret", 0)
  Debug.Printf("Hijack_PLZ45: Reset main_turret yaw: " .. tostring(bResult))
  bResult = Vehicle.EnableTurret(self._vehicle, "main_turret", false, "all", true)
  Debug.Printf("Hijack_PLZ45: Disable main_turret  : " .. tostring(bResult))
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
        nTime = 0.73,
        tControllerRumble = {nlength = 0.2},
        tCameraShake = {fSetCameraAmplitude = 1, fSetCameraShake = 0.1}
      }
    },
    miniGameStartDelay = 2.3,
    miniGame = {
      nTimeOut = 1,
      sAction = "press",
      bExtraHudParameters = true,
      nTranslucency = 255,
      bShowTimer = false,
      nHudButtonMotionSpeed = 0.25,
      nXPosition = 0,
      nYPosition = -0.45,
      button = Controller.RPad_Up
    },
    OnFailureAnimationBegin = OnFailureEvents,
    tFailureMultiEvents = {
      {
        nTime = 0,
        tControllerRumble = {nlength = 0.5},
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
    tMultiEvents = {
      {
        nTime = 0,
        tControllerRumble = {nlength = 0.2},
        tCameraShake = {fSetCameraAmplitude = 2.5, fSetCameraShake = 0.1}
      },
      {
        nTime = 1.33,
        tControllerRumble = {nlength = 0.2},
        tCameraShake = {fSetCameraAmplitude = 2, fSetCameraShake = 0.1}
      }
    },
    miniGameStartDelay = 1.1,
    miniGame = {
      nTimeOut = 1,
      sAction = "press",
      bExtraHudParameters = true,
      nTranslucency = 255,
      bShowTimer = false,
      nHudButtonMotionSpeed = 0.25,
      nXPosition = 0,
      nYPosition = -0.45,
      sAction = "press",
      button = Controller.LStick_Up,
      nScale = 1.2
    },
    OnFailureAnimationBegin = OnFailureEvents,
    tFailureMultiEvents = {
      {
        nTime = 1,
        tControllerRumble = {nlength = 0.2},
        tCameraShake = {fSetCameraAmplitude = 2, fSetCameraShake = 0.1}
      },
      {
        nTime = 1.96,
        tControllerRumble = {nlength = 0.5},
        tCameraShake = {fSetCameraAmplitude = 1, fSetCameraShake = 0.1}
      }
    }
  }
  self[3] = {
    hijackerAnimation = "ActionHijackHijackerC",
    hijackeeAnimation = "ActionHijackHijackeeC",
    vehicleAnimation = door_anim_C,
    tMultiEvents = {
      {
        nTime = 0.33,
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
