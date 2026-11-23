import("MrxActionHijack")
local nSection = {}
local door_anim_A = "all_AMX30_driverhatch_actionhijack_getin_section01_fb"
local door_anim_B = "all_AMX30_driverhatch_actionhijack_getin_section02_fb"
local door_anim_C = "all_AMX30_driverhatch_actionhijack_getin_section03_fb"
local door_anim_D = "all_AMX30_driverhatch_actionhijack_getin_section04_fb"
local door_anim_E = "all_AMX30_driverhatch_actionhijack_getin_section05_fb"
local door_anim_F = "all_AMX30_driverhatch_actionhijack_getin_section06_fb"
local door_anim_CFail = "all_AMX30_driverhatch_actionhijack_getin_section03failure_fb"
local door_anim_DLoopFail = "all_AMX30_driverhatch_actionhijack_getin_section04loopfailure_fb"
local door_anim_DLoopA = "all_AMX30_driverhatch_actionhijack_getin_section04loopa_fb"
local door_anim_DLoopB = "all_AMX30_driverhatch_actionhijack_getin_section04loopb_fb"
local door_anim_DLoopC = "all_AMX30_driverhatch_actionhijack_getin_section04loopc_fb"
local door_anim_DLoopD = "all_AMX30_driverhatch_actionhijack_getin_section04loopd_fb"
local door_anim_DLoopE = "all_AMX30_driverhatch_actionhijack_getin_section04loopf_fb"
local door_anim_DLoopF = "all_AMX30_driverhatch_actionhijack_getin_section04loopg_fb"
local door_anim_DLoopG = "all_AMX30_driverhatch_actionhijack_getin_section04looph_fb"
local door_anim_DLoopH = "all_AMX30_driverhatch_actionhijack_getin_section04loopi_fb"
local uGunTemplate, uGunInstance, uGrenadeTemplate, uGrenadeInstance

function Init()
  Debug.Printf("Hijack_AMX30: Loading vehicle animation assets")
  local assetType = "animation"
  Pg.LoadAsset(door_anim_A, assetType)
  Pg.LoadAsset(door_anim_B, assetType)
  Pg.LoadAsset(door_anim_C, assetType)
  Pg.LoadAsset(door_anim_D, assetType)
  Pg.LoadAsset(door_anim_E, assetType)
  Pg.LoadAsset(door_anim_F, assetType)
  Pg.LoadAsset(door_anim_CFail, assetType)
  Pg.LoadAsset(door_anim_DLoopFail, assetType)
  Pg.LoadAsset(door_anim_DLoopA, assetType)
  Pg.LoadAsset(door_anim_DLoopB, assetType)
  Pg.LoadAsset(door_anim_DLoopC, assetType)
  Pg.LoadAsset(door_anim_DLoopD, assetType)
  Pg.LoadAsset(door_anim_DLoopE, assetType)
  Pg.LoadAsset(door_anim_DLoopF, assetType)
  Pg.LoadAsset(door_anim_DLoopG, assetType)
  Pg.LoadAsset(door_anim_DLoopH, assetType)
  Pg.LoadAsset("Global_weapon_pistol", "model")
  Pg.LoadAsset("global_weapon_fraggrenade", "model")
end

function Deinit()
  Debug.Printf("Hijack_AMX30: Unloading vehicle animation assets")
  local assetType = "animation"
  Pg.UnloadAsset(door_anim_A, assetType)
  Pg.UnloadAsset(door_anim_B, assetType)
  Pg.UnloadAsset(door_anim_C, assetType)
  Pg.UnloadAsset(door_anim_D, assetType)
  Pg.UnloadAsset(door_anim_E, assetType)
  Pg.UnloadAsset(door_anim_F, assetType)
  Pg.UnloadAsset(door_anim_CFail, assetType)
  Pg.UnloadAsset(door_anim_DLoopFail, assetType)
  Pg.UnloadAsset(door_anim_DLoopA, assetType)
  Pg.UnloadAsset(door_anim_DLoopB, assetType)
  Pg.UnloadAsset(door_anim_DLoopC, assetType)
  Pg.UnloadAsset(door_anim_DLoopD, assetType)
  Pg.UnloadAsset(door_anim_DLoopE, assetType)
  Pg.UnloadAsset(door_anim_DLoopF, assetType)
  Pg.UnloadAsset(door_anim_DLoopG, assetType)
  Pg.UnloadAsset(door_anim_DLoopH, assetType)
  Pg.UnloadAsset("Global_weapon_pistol", "model")
  Pg.UnloadAsset("global_weapon_fraggrenade", "model")
end

function StartHijack(hijackerObject, hijackeeObject, seatObject, vehicleObject)
  local bResult, sBone
  uGunTemplate = Pg.GetGuidByName("Action Hijack Prop (Pistol)")
  sBone = "bone_pistol"
  bResult, uGunInstance = Object.Attach(vehicleObject, sBone, uGunTemplate)
  Debug.Printf("StartHijack: attaching pistol: " .. tostring(bResult) .. "  " .. tostring(uGunInstance))
  uGrenadeTemplate = Pg.GetGuidByName("Action Hijack Prop (Grenade)")
  sBone = "bone_grenade"
  bResult, uGrenadeInstance = Object.Attach(vehicleObject, sBone, uGrenadeTemplate)
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
  FaceState_anim_BLoopA = {
    hijacker = {
      faceStateA = {state = "angry", weight = 1}
    },
    hijackee = {
      faceStateA = {state = "Scared", weight = 1}
    }
  }
  FaceState_anim_BLoopB = {
    hijackee = {
      faceStateA = {state = "Scared", weight = 0.75}
    }
  }
  FaceState_anim_BLoopC = {
    hijackee = {
      faceStateA = {state = "Scared", weight = 0.25},
      faceStateB = {state = "EyesShut", weight = 1},
      faceStateC = {state = "angry", weight = 0}
    }
  }
  FaceState_anim_BLoopD = {
    hijackee = {
      faceStateA = {state = "Scared", weight = 0},
      faceStateB = {state = "EyesShut", weight = 0},
      faceStateC = {state = "angry", weight = 0.25}
    }
  }
  FaceState_anim_BLoopF = {
    hijackee = {
      faceStateA = {state = "angry", weight = 0.5},
      faceStateB = {state = "Scared", weight = 0}
    }
  }
  FaceState_anim_BLoopG = {
    hijackee = {
      faceStateA = {state = "angry", weight = 0.75}
    }
  }
  FaceState_anim_BLoopH = {
    hijackee = {
      faceStateA = {state = "angry", weight = 1}
    }
  }
  self[1] = {
    hijackerAnimation = "ActionHijackHijackerA",
    hijackeeAnimation = "ActionHijackHijackeeA",
    vehicleAnimation = door_anim_A,
    tCharactersFaceStates = {
      hijacker = {
        faceStateA = {state = "angry", weight = 0.5}
      }
    },
    tMultiEvents = {
      {
        nTime = 0.1,
        tControllerRumble = {nlength = 0.2},
        tCameraShake = {fSetCameraAmplitude = 2.5, fSetCameraShake = 0.1}
      }
    }
  }
  self[2] = {
    hijackerAnimation = "ActionHijackHijackerB",
    hijackeeAnimation = "ActionHijackHijackeeB",
    vehicleAnimation = door_anim_B
  }
  self[3] = {
    hijackerAnimation = "ActionHijackHijackerC",
    hijackeeAnimation = "ActionHijackHijackeeC",
    hijackerAnimationFail = "ActionHijackHijackerFailC",
    hijackeeAnimationFail = "ActionHijackHijackeeFailC",
    vehicleAnimation = door_anim_C,
    vehicleAnimationFail = door_anim_CFail,
    tCharactersFaceStates = {
      hijacker = {
        faceStateA = {state = "angry", weight = 0.75}
      },
      hijackee = {
        faceStateA = {state = "angry", weight = 0.75}
      },
      hijackerFail = {
        faceStateA = {state = "angry", weight = 0},
        faceStateB = {state = "Scared", weight = 1}
      },
      hijackeeFail = {
        faceStateA = {state = "angry", weight = 1}
      }
    },
    OnFailureAnimationBegin = OnFailureAnimationBegin,
    miniGameStartDelay = 3.46,
    miniGame = {
      bExtraHudParameters = true,
      nXPosition = 0,
      nYPosition = 0.8,
      nTranslucency = 255,
      bShowTimer = false,
      nHudButtonMotionSpeed = 0.2,
      nTimeOut = 1,
      sAction = "press",
      button = Controller.RPad_Down
    },
    tMultiEvents = {
      {
        nTime = 0.7,
        tControllerRumble = {nlength = 0.2},
        tCameraShake = {fSetCameraAmplitude = 1, fSetCameraShake = 0.1}
      },
      {
        nTime = 3.3,
        tControllerRumble = {nlength = 0.2},
        tCameraShake = {fSetCameraAmplitude = 2.5, fSetCameraShake = 0.1}
      }
    },
    OnFailureAnimationBegin = OnFailureEvents,
    tFailureMultiEvents = {
      {
        nTime = 0.26,
        tControllerRumble = {nlength = 0.5}
      }
    }
  }
  self[4] = {
    hijackerAnimation = "ActionHijackHijackerD",
    hijackeeAnimation = "ActionHijackHijackeeD",
    vehicleAnimation = door_anim_D,
    tCharactersFaceStates = {
      hijackee = {
        faceStateA = {state = "angry", weight = 0},
        faceStateB = {state = "Scared", weight = 0.75}
      }
    },
    tMultiEvents = {
      {
        nTime = 0.16,
        tControllerRumble = {nlength = 0.2},
        tCameraShake = {fSetCameraAmplitude = 1.5, fSetCameraShake = 0.1}
      }
    }
  }
  self[5] = {
    OnFailureAnimationBegin = RemoveWeapons,
    hijackerAnimationFail = "ActionHijackHijackerFailD",
    hijackeeAnimationFail = "ActionHijackHijackeeFailD",
    vehicleAnimationFail = door_anim_DLoopFail,
    tCharactersFaceStates = {
      hijackerFail = {
        faceStateA = {state = "angry", weight = 0},
        faceStateB = {state = "Scared", weight = 1}
      },
      hijackeeFail = {
        faceStateA = {state = "angry", weight = 1}
      }
    },
    OnFailureAnimationBegin = OnFailureControlRumble,
    nReactiveLoop = 3,
    tHijackerAnimations = {
      "ActionHijackHijackerDLoopH",
      "ActionHijackHijackerDLoopG",
      "ActionHijackHijackerDLoopF",
      "ActionHijackHijackerDLoopE",
      "ActionHijackHijackerDLoopD",
      "ActionHijackHijackerDLoopC",
      "ActionHijackHijackerDLoopB",
      "ActionHijackHijackerDLoopA"
    },
    tHijackeeAnimations = {
      "ActionHijackHijackeeBLoopH",
      "ActionHijackHijackeeBLoopG",
      "ActionHijackHijackeeBLoopF",
      "ActionHijackHijackeeBLoopE",
      "ActionHijackHijackeeBLoopD",
      "ActionHijackHijackeeBLoopC",
      "ActionHijackHijackeeBLoopB",
      "ActionHijackHijackeeBLoopA"
    },
    tVehicleAnimations = {
      door_anim_DLoopH,
      door_anim_DLoopG,
      door_anim_DLoopF,
      door_anim_DLoopE,
      door_anim_DLoopD,
      door_anim_DLoopC,
      door_anim_DLoopB,
      door_anim_DLoopA
    },
    tReactiveLoopFaceStates = {
      FaceState_anim_BLoopH,
      FaceState_anim_BLoopG,
      FaceState_anim_BLoopF,
      FaceState_anim_BLoopD,
      FaceState_anim_BLoopC,
      FaceState_anim_BLoopB,
      FaceState_anim_BLoopA
    },
    miniGameStartDelay = 0.01,
    miniGame = {
      bExtraHudParameters = true,
      nXPosition = -0.5,
      nYPosition = 0.2,
      nTranslucency = 255,
      nTimeOut = 35,
      bShowTimer = false,
      sAction = "tap",
      nHudButtonMotionSpeed = 0.1,
      button = Controller.RPad_Left,
      nDriverDifficulty = 1.4,
      nSuccessThreshold = 1
    },
    OnFailureAnimationBegin = OnFailureEvents,
    tFailureMultiEvents = {
      {
        nTime = 0.76,
        tControllerRumble = {nlength = 0.5}
      }
    }
  }
  self[6] = {
    hijackerAnimation = "ActionHijackHijackerE",
    hijackeeAnimation = "ActionHijackHijackeeE",
    vehicleAnimation = door_anim_E,
    tCharactersFaceStates = {
      hijackee = {
        faceStateA = {state = "Scared", weight = 1}
      }
    },
    tMultiEvents = {
      {
        nTime = 0.16,
        tControllerRumble = {nlength = 0.2},
        tCameraShake = {fSetCameraAmplitude = 1.5, fSetCameraShake = 0.1}
      }
    }
  }
  self[7] = {
    hijackerAnimation = "ActionHijackHijackerF",
    hijackeeAnimation = "ActionHijackHijackeeF",
    vehicleAnimation = door_anim_F,
    nTankHijackExplosion = {
      nDelay = 2.1,
      uObject = self._vehicle,
      sHardpoint = "hp_seat_lt",
      fSetCameraAmplitude = 10,
      fSetCameraShake = 2.5,
      fSetRumbleLength = 1.5
    },
    tMultiEvents = {
      {
        nTime = 0.1,
        tControllerRumble = {nlength = 0.2},
        tCameraShake = {fSetCameraAmplitude = 1.5, fSetCameraShake = 0.1}
      }
    },
    bDriverDoneRemove = true
  }
  MrxActionHijack.Begin(self, 1)
  return self
end

function RemoveWeapons(self)
  local bResult
  bResult = Object.Detach(self._vehicle, uGunInstance)
  Debug.Printf("Hijack_AMX30: RemoveWeapons: detach gun: " .. tostring(bResult))
  bResult = Object.Remove(uGunInstance)
  Debug.Printf("Hijack_AMX30: RemoveWeapons: remove gun: " .. tostring(bResult))
  bResult = Object.Detach(self._vehicle, uGrenadeInstance)
  Debug.Printf("Hijack_AMX30: RemoveWeapons: detach grenade: " .. tostring(bResult))
  bResult = Object.Remove(uGrenadeInstance)
  Debug.Printf("Hijack_AMX30: RemoveWeapons: remove grenade: " .. tostring(bResult))
end

function OnFailureAnimationBegin(self, nCurrent)
  Debug.Printf("OnFailureAnimationBegin: nCurrent: " .. tostring(nCurrent))
  if nCurrent == 3 then
    Debug.Printf("OnFailureAnimationBegin: queue gunshot VFX")
    Event.Create(Event.TimerRelative, {0.05}, function(self)
      Debug.Printf("OnFailureAnimationBegin: VFX")
      local x, y, z = Object.GetHardpointPosition(uGunInstance, "hp_barreltip_a")
      Debug.Printf("OnFailureAnimationBegin: x, y, z: " .. tostring(x) .. ", " .. tostring(y) .. ", " .. tostring(z))
      if x and y and z then
        Pg.Spawn("global_particle_muzzleflash_handgun", x, y, z, 0)
        Debug.Printf("OnFailureAnimationBegin: spawned global_particle_muzzleflash_handgun")
        Pg.Spawn("global_particle_shellhandgun", x, y, z, 0)
        Debug.Printf("OnFailureAnimationBegin: spawned global_particle_shellhandgun")
      end
      Pg.Rumble(self._hijacker, 0.3)
    end, {self})
  end
end

function OnFailureControlRumble(self, nCurrent)
  Debug.Printf("OnFailureAnimationBegin: nCurrent: " .. tostring(nCurrent))
  if nCurrent == 5 then
    Pg.Rumble(self._hijacker, 1)
  end
end

function OnFailureEvents(self, nCurrent)
  Debug.Printf("OnFailureAnimationBegin: nCurrent: " .. tostring(nCurrent))
  local tFailEvents = self[self.nCurrent].tFailureMultiEvents
  local oLocalPlayer = self._hijacker
  MrxActionHijack._ProcessMultiEventTable(oLocalPlayer, tFailEvents)
end
