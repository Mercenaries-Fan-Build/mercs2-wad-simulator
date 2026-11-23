import("MrxActionHijack")
local door_anim_A = "all_WZ551_driverhatch_actionhijack_getin_section01_fb"
local door_anim_B = "all_WZ551_driverhatch_actionhijack_getin_section02_fb"
local door_anim_AFail = "all_WZ551_driverhatch_actionhijack_getin_section01failure_fb"
local uGunTemplate, uGunInstance

function Init()
  Debug.Printf("Hijack_WZ551: Loading vehicle animation assets")
  local assetType = "animation"
  Pg.LoadAsset(door_anim_A, assetType)
  Pg.LoadAsset(door_anim_B, assetType)
  Pg.LoadAsset(door_anim_AFail, assetType)
  Pg.LoadAsset("Global_weapon_pistol", "model")
end

function Deinit()
  Debug.Printf("Hijack_WZ551: Unloading vehicle animation assets")
  local assetType = "animation"
  Pg.UnloadAsset(door_anim_A, assetType)
  Pg.UnloadAsset(door_anim_B, assetType)
  Pg.UnloadAsset(door_anim_AFail, assetType)
  Pg.UnloadAsset("Global_weapon_pistol", "model")
end

function StartHijack(hijackerObject, hijackeeObject, seatObject, vehicleObject)
  local bResult, sBone
  uGunTemplate = Pg.GetGuidByName("Action Hijack Prop (Pistol)")
  sBone = "bone_pistol"
  bResult, uGunInstance = Object.Attach(vehicleObject, sBone, uGunTemplate)
  Debug.Printf("StartHijack: attaching pistol: " .. tostring(bResult) .. "  " .. tostring(uGunInstance))
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
  self._OnActionHijackComplete = RemoveWeapons
  MrxActionHijack.InitializeActionHijack(self)
  MrxActionHijack.TankPrep(self)
  self[1] = {
    hijackerAnimation = "ActionHijackHijackerA",
    hijackeeAnimation = "ActionHijackHijackeeA",
    vehicleAnimation = door_anim_A,
    hijackerAnimationFail = "ActionHijackHijackerFailA",
    hijackeeAnimationFail = "ActionHijackHijackeeFailA",
    vehicleAnimationFail = door_anim_AFail,
    miniGameStartDelay = 0.76,
    miniGame = {
      nTimeOut = 1,
      bExtraHudParameters = true,
      nXPosition = 0.5,
      nYPosition = 0.2,
      nTranslucency = 255,
      bShowTimer = false,
      sAction = "press",
      button = Controller.RPad_Right,
      nHudButtonMotionSpeed = 0.2
    },
    tMultiEvents = {
      {
        nTime = 0.46,
        tControllerRumble = {nlength = 0.2},
        tCameraShake = {fSetCameraAmplitude = 1, fSetCameraShake = 0.1}
      },
      {
        nTime = 1.1,
        tParticleEfx = {
          objectInstanceTemplate = uGunInstance,
          objectHardPointBoneName = "hp_barreltip_a",
          sPFXname = "global_particle_muzzleflash_handgun"
        },
        tControllerRumble = {nlength = 0.2},
        tCameraShake = {fSetCameraAmplitude = 1, fSetCameraShake = 0.1}
      },
      {
        nTime = 1.13,
        tParticleEfx = {
          objectInstanceTemplate = uGunInstance,
          objectHardPointBoneName = "hp_barreltip_a",
          sPFXname = "global_particle_shellhandgun"
        }
      }
    },
    OnFailureAnimationBegin = OnFailureEvents,
    tFailureMultiEvents = {
      {
        nTime = 0.53,
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
        nTime = 0.96,
        tControllerRumble = {nlength = 0.2},
        tCameraShake = {fSetCameraAmplitude = 2, fSetCameraShake = 0.1}
      },
      {
        nTime = 1.6,
        tControllerRumble = {nlength = 0.2},
        tCameraShake = {fSetCameraAmplitude = 0.5, fSetCameraShake = 0.1}
      },
      {
        nTime = 2,
        tControllerRumble = {nlength = 0.2},
        tCameraShake = {fSetCameraAmplitude = 1, fSetCameraShake = 0.1}
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
