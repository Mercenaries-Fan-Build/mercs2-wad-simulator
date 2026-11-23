import("MrxActionHijack")
local door_anim_A = "All_BankTruck_driverdoor_actionhijack_getin_section01_fb"
local door_anim_AFail = "All_BankTruck_driverdoor_actionhijack_getin_section01failure_fb"
local door_anim_B = "All_BankTruck_driverdoor_actionhijack_getin_section02_fb"
local door_anim_BFail = "All_BankTruck_driverdoor_actionhijack_getin_section02failure_fb"
local door_anim_C = "All_BankTruck_driverdoor_actionhijack_getin_section03_fb"

function Init()
  Debug.Printf("Hijack_BankTruck: Loading vehicle animation assets")
  local assetType = "animation"
  Pg.LoadAsset(door_anim_A, assetType)
  Pg.LoadAsset(door_anim_AFail, assetType)
  Pg.LoadAsset(door_anim_B, assetType)
  Pg.LoadAsset(door_anim_BFail, assetType)
  Pg.LoadAsset(door_anim_C, assetType)
end

function Deinit()
  Debug.Printf("Hijack_BankTruck: Unloading vehicle animation assets")
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
        tCameraShake = {fSetCameraAmplitude = 1, fSetCameraShake = 0.1}
      },
      {
        nTime = 3.56,
        tControllerRumble = {nlength = 0.2},
        tCameraShake = {fSetCameraAmplitude = 2.5, fSetCameraShake = 0.2},
        tParticleEfx = {
          objectInstanceTemplate = self._hijacker,
          objectHardPointBoneName = "Bone_RFootBone1",
          sPFXname = "global_particle_shatteringGlass_vehicle"
        }
      }
    },
    miniGameStartDelay = 2.8,
    miniGame = {
      nTimeOut = 2,
      sAction = "press",
      bExtraHudParameters = true,
      nTranslucency = 255,
      bShowTimer = false,
      nHudButtonMotionSpeed = 0.2,
      nXPosition = -0.5,
      nYPosition = 0.2,
      button = Controller.RPad_Left
    },
    OnFailureAnimationBegin = OnFailureEvents,
    tFailureMultiEvents = {
      {
        nTime = 1.1,
        tControllerRumble = {nlength = 0.2},
        tCameraShake = {fSetCameraAmplitude = 2, fSetCameraShake = 0.1}
      },
      {
        nTime = 2.2,
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
        nTime = 0.5,
        tControllerRumble = {nlength = 0.2},
        tCameraShake = {fSetCameraAmplitude = 2, fSetCameraShake = 0.1}
      }
    },
    miniGameStartDelay = 0.5,
    miniGame = {
      nTimeOut = 0.7,
      sAction = "press",
      bExtraHudParameters = true,
      nTranslucency = 255,
      bShowTimer = false,
      nHudButtonMotionSpeed = 0.2,
      nYPosition = 0.8,
      nXPosition = 0,
      button = Controller.RPad_Down
    },
    OnFailureAnimationBegin = OnFailureEvents,
    tFailureMultiEvents = {
      {
        nTime = 1.1,
        tControllerRumble = {nlength = 0.2},
        tCameraShake = {fSetCameraAmplitude = 2, fSetCameraShake = 0.1}
      },
      {
        nTime = 2.2,
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
        nTime = 0.76,
        tControllerRumble = {nlength = 0.1},
        tCameraShake = {fSetCameraAmplitude = 2, fSetCameraShake = 0.1}
      },
      {
        nTime = 1.43,
        tControllerRumble = {nlength = 0.1},
        tCameraShake = {fSetCameraAmplitude = 2, fSetCameraShake = 0.1}
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
