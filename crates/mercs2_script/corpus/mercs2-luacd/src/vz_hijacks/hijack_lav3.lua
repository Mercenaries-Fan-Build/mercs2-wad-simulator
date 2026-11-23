import("MrxActionHijack")
local door_anim_A = "all_LAV3_driverdoor_actionhijack_getin_section01_fb"
local door_anim_AFail = "all_LAV3_driverdoor_actionhijack_getin_failsection01_fb"
local door_anim_B = "all_LAV3_driverdoor_actionhijack_getin_section02_fb"

function Init()
  Debug.Printf("Hijack_LAV3: Loading vehicle animation assets")
  local assetType = "animation"
  Pg.LoadAsset(door_anim_A, assetType)
  Pg.LoadAsset(door_anim_AFail, assetType)
  Pg.LoadAsset(door_anim_B, assetType)
end

function Deinit()
  Debug.Printf("Hijack_LAV3: Unloading vehicle animation assets")
  local assetType = "animation"
  Pg.UnloadAsset(door_anim_A, assetType)
  Pg.UnloadAsset(door_anim_AFail, assetType)
  Pg.UnloadAsset(door_anim_B, assetType)
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
    miniGameStartDelay = 0.66,
    miniGame = {
      bExtraHudParameters = true,
      nTranslucency = 255,
      bShowTimer = false,
      nHudButtonMotionSpeed = 0.25,
      nScale = 1.2,
      nXPosition = 0,
      nYPosition = 0.7,
      nTimeOut = 2,
      sAction = "press",
      button = Controller.LStick_Down,
      nScale = 1.2
    },
    tMultiEvents = {
      {
        nTime = 0,
        tCameraParams = {
          nStartNear = 0,
          nEndNear = 1,
          nStartFar = 6,
          nEndFar = 8
        }
      },
      {
        nTime = 0.26,
        tControllerRumble = {nlength = 0.2},
        tCameraShake = {fSetCameraAmplitude = 1, fSetCameraShake = 0.1}
      },
      {
        nTime = 1.7,
        tControllerRumble = {nlength = 0.2},
        tCameraShake = {fSetCameraAmplitude = 1, fSetCameraShake = 0.1}
      }
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
  self[2] = {
    hijackerAnimation = "ActionHijackHijackerB",
    hijackeeAnimation = "ActionHijackHijackeeB",
    vehicleAnimation = door_anim_B,
    tMultiEvents = {
      {
        nTime = 0.13,
        tControllerRumble = {nlength = 0.2},
        tCameraShake = {fSetCameraAmplitude = 2, fSetCameraShake = 0.1}
      },
      {
        nTime = 2.93,
        tControllerRumble = {nlength = 0.2},
        tCameraShake = {fSetCameraAmplitude = 2, fSetCameraShake = 0.1}
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
