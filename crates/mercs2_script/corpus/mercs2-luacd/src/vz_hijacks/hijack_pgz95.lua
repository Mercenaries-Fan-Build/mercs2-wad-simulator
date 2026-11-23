import("MrxActionHijack")
local door_anim_A = "all_PGZ95_driverdoor_actionhijack_getin_section01_fb"
local door_anim_AFail = "all_PGZ95_driverdoor_actionhijack_getin_failsection01_fb"
local door_anim_B = "all_PGZ95_driverdoor_actionhijack_getin_section02_fb"
local door_anim_BFail = "all_PGZ95_driverdoor_actionhijack_getin_failsection02_fb"
local door_anim_C = "all_PGZ95_driverdoor_actionhijack_getin_section03_fb"

function Init()
  Debug.Printf("Hijack_PGZ95: Loading vehicle animation assets")
  local assetType = "animation"
  Pg.LoadAsset(door_anim_A, assetType)
  Pg.LoadAsset(door_anim_AFail, assetType)
  Pg.LoadAsset(door_anim_B, assetType)
  Pg.LoadAsset(door_anim_BFail, assetType)
  Pg.LoadAsset(door_anim_C, assetType)
end

function Deinit()
  Debug.Printf("Hijack_PGZ95: Unloading vehicle animation assets")
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
  local bResult = Vehicle.SetTurretYaw(self._vehicle, "main_turret", -0.2)
  Debug.Printf("Hijack_PGZ95: Reset main_turret yaw: " .. tostring(bResult))
  bResult = Vehicle.EnableTurret(self._vehicle, "main_turret", false, "all", true)
  Debug.Printf("Hijack_PGZ95: Disable main_turret  : " .. tostring(bResult))
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
        nTime = 0.76,
        tControllerRumble = {nlength = 0.2},
        tCameraShake = {fSetCameraAmplitude = 1, fSetCameraShake = 0.1}
      }
    },
    miniGameStartDelay = 2.55,
    miniGame = {
      nTimeOut = 0.75,
      sAction = "press",
      bExtraHudParameters = true,
      nTranslucency = 255,
      bShowTimer = false,
      nHudButtonMotionSpeed = 0.2,
      nXPosition = 0,
      nYPosition = 0.8,
      button = Controller.RPad_Down
    },
    OnFailureAnimationBegin = OnFailureEvents,
    tFailureMultiEvents = {
      {
        nTime = 0,
        tControllerRumble = {nlength = 0.2},
        tCameraShake = {fSetCameraAmplitude = 2.5, fSetCameraShake = 0.1}
      },
      {
        nTime = 1.733,
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
        nTime = 1.333,
        tControllerRumble = {nlength = 0.2},
        tCameraShake = {fSetCameraAmplitude = 2.5, fSetCameraShake = 0.1}
      }
    },
    miniGameStartDelay = 1.63,
    miniGame = {
      nTimeOut = 0.5,
      sAction = "press",
      bExtraHudParameters = true,
      nTranslucency = 255,
      bShowTimer = false,
      nHudButtonMotionSpeed = 0.25,
      nXPosition = 0,
      nYPosition = 0.8,
      sAction = "press",
      button = Controller.LStick_Down,
      nScale = 1.2
    },
    OnFailureAnimationBegin = OnFailureEvents,
    tFailureMultiEvents = {
      {
        nTime = 0.33,
        tControllerRumble = {nlength = 0.2},
        tCameraShake = {fSetCameraAmplitude = 1, fSetCameraShake = 0.1}
      },
      {
        nTime = 1.3,
        tControllerRumble = {nlength = 0.5},
        tCameraShake = {fSetCameraAmplitude = 2, fSetCameraShake = 0.1}
      }
    }
  }
  self[3] = {
    hijackerAnimation = "ActionHijackHijackerC",
    hijackeeAnimation = "ActionHijackHijackeeC",
    vehicleAnimation = door_anim_C,
    tMultiEvents = {
      {
        nTime = 0.26,
        tControllerRumble = {nlength = 0.2},
        tCameraShake = {fSetCameraAmplitude = 1, fSetCameraShake = 0.1}
      },
      {
        nTime = 2.56,
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
