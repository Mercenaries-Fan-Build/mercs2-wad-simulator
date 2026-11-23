import("MrxActionHijack")
local door_anim_A = "all_M1A2_driverhatch_actionhijack_getin_section01_fb"
local door_anim_B = "all_M1A2_driverhatch_actionhijack_getin_section02_fb"
local door_anim_C = "all_M1A2_driverhatch_actionhijack_getin_section03_fb"
local door_anim_D = "all_M1A2_driverhatch_actionhijack_getin_section04_fb"
local door_anim_E = "all_M1A2_driverhatch_actionhijack_getin_section05_fb"
local door_anim_F = "all_M1A2_driverhatch_actionhijack_getin_section06_fb"
local door_anim_BFail = "all_M1A2_driverhatch_actionhijack_getin_section02failure_fb"
local door_anim_CFail = "all_M1A2_driverhatch_actionhijack_getin_section03failure_fb"
local door_anim_DLoopFail = "all_M1A2_driverhatch_actionhijack_getin_section04loopfailure_fb"
local door_anim_EFail = "all_M1A2_driverhatch_actionhijack_getin_section05failure_fb"
local door_anim_DLoopA = "all_M1A2_driverhatch_actionhijack_getin_section04loopa_fb"
local door_anim_DLoopB = "all_M1A2_driverhatch_actionhijack_getin_section04loopb_fb"
local door_anim_DLoopC = "all_M1A2_driverhatch_actionhijack_getin_section04loopc_fb"
local door_anim_DLoopD = "all_M1A2_driverhatch_actionhijack_getin_section04loopd_fb"
local door_anim_DLoopE = "all_M1A2_driverhatch_actionhijack_getin_section04loopf_fb"
local door_anim_DLoopF = "all_M1A2_driverhatch_actionhijack_getin_section04loopg_fb"
local door_anim_DLoopG = "all_M1A2_driverhatch_actionhijack_getin_section04looph_fb"
local door_anim_DLoopH = "all_M1A2_driverhatch_actionhijack_getin_section04loopi_fb"
local nSection = {}
local uGunTemplate, uGunInstance, uGrenadeTemplate, uGrenadeInstance

function Init()
  local assetType = "animation"
  Pg.LoadAsset(door_anim_A, assetType)
  Pg.LoadAsset(door_anim_B, assetType)
  Pg.LoadAsset(door_anim_C, assetType)
  Pg.LoadAsset(door_anim_D, assetType)
  Pg.LoadAsset(door_anim_E, assetType)
  Pg.LoadAsset(door_anim_F, assetType)
  Pg.LoadAsset(door_anim_BFail, assetType)
  Pg.LoadAsset(door_anim_CFail, assetType)
  Pg.LoadAsset(door_anim_DLoopFail, assetType)
  Pg.LoadAsset(door_anim_EFail, assetType)
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
  local assetType = "animation"
  Pg.UnloadAsset(door_anim_A, assetType)
  Pg.UnloadAsset(door_anim_B, assetType)
  Pg.UnloadAsset(door_anim_C, assetType)
  Pg.UnloadAsset(door_anim_D, assetType)
  Pg.UnloadAsset(door_anim_E, assetType)
  Pg.UnloadAsset(door_anim_F, assetType)
  Pg.UnloadAsset(door_anim_BFail, assetType)
  Pg.UnloadAsset(door_anim_CFail, assetType)
  Pg.UnloadAsset(door_anim_DLoopFail, assetType)
  Pg.UnloadAsset(door_anim_EFail, assetType)
  Pg.UnloadAsset(door_anim_DLoopA, assetType)
  Pg.UnloadAsset(door_anim_DLoopB, assetType)
  Pg.UnloadAsset(door_anim_DLoopC, assetType)
  Pg.UnloadAsset(door_anim_DLoopD, assetType)
  Pg.UnloadAsset(door_anim_DLoopE, assetType)
  Pg.UnloadAsset(door_anim_DLoopF, assetType)
  Pg.UnloadAsset(door_anim_DLoopG, assetType)
  Pg.UnloadAsset(door_anim_DLoopH, assetType)
  Pg.UnloadAsset("Global_weapon_pistol", "model")
  Pg.LoadAsset("global_weapon_fraggrenade", "model")
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
  self._OnActionHijackComplete = OnActionHijackComplete
  MrxActionHijack.InitializeActionHijack(self)
  MrxActionHijack.TankPrep(self)
  FaceState_anim_BLoopA = {
    hijacker = {
      faceStateA = {state = "angry", weight = 1}
    },
    hijackee = {
      faceStateA = {state = "Scared", weight = 0},
      faceStateB = {state = "angry", weight = 1}
    }
  }
  FaceState_anim_BLoopB = {
    hijackee = {
      faceStateA = {state = "Scared", weight = 0},
      faceStateB = {state = "angry", weight = 0.75}
    }
  }
  FaceState_anim_BLoopC = {
    hijackee = {
      faceStateA = {state = "Scared", weight = 0.5},
      faceStateB = {state = "EyesShut", weight = 1},
      faceStateC = {state = "angry", weight = 0}
    }
  }
  FaceState_anim_BLoopD = {
    hijackee = {
      faceStateA = {state = "Scared", weight = 0.25},
      faceStateB = {state = "EyesShut", weight = 0},
      faceStateC = {state = "angry", weight = 0}
    }
  }
  FaceState_anim_BLoopE = {
    hijackee = {
      faceStateA = {state = "angry", weight = 0.25},
      faceStateB = {state = "Scared", weight = 0}
    }
  }
  FaceState_anim_BLoopF = {
    hijackee = {
      faceStateA = {state = "angry", weight = 0.5}
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
    hijackerAnimationFail = "ActionHijackHijackerFailB",
    vehicleAnimation = door_anim_B,
    vehicleAnimationFail = door_anim_BFail,
    miniGameStartDelay = 1.13,
    miniGame = {
      nTimeOut = 1,
      bExtraHudParameters = true,
      nTranslucency = 255,
      nXPosition = -0.5,
      nYPosition = 0.2,
      nScale = 1.2,
      bShowTimer = false,
      sAction = "press",
      button = Controller.LStick_Left,
      nHudButtonMotionSpeed = 0.25
    },
    OnFailureAnimationBegin = OnFailureEvents,
    tFailureMultiEvents = {
      {
        nTime = 0.2,
        tControllerRumble = {nlength = 0.5},
        tCameraShake = {fSetCameraAmplitude = 2.5, fSetCameraShake = 0.1}
      }
    }
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
    miniGameStartDelay = 3.58,
    miniGame = {
      nTimeOut = 0.75,
      bExtraHudParameters = true,
      nTranslucency = 255,
      nXPosition = 0,
      nYPosition = -0.4,
      bShowTimer = false,
      sAction = "press",
      button = Controller.RPad_Up,
      nHudButtonMotionSpeed = 0.2
    },
    tMultiEvents = {
      {
        nTime = 0.73,
        tControllerRumble = {nlength = 0.2},
        tCameraShake = {fSetCameraAmplitude = 1.5, fSetCameraShake = 0.1}
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
    OnFailureAnimationBegin = RemoveWeapons,
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
    OnFailureAnimationBegin = OnFailureControlRumble,
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
      FaceState_anim_BLoopE,
      FaceState_anim_BLoopD,
      FaceState_anim_BLoopC,
      FaceState_anim_BLoopB,
      FaceState_anim_BLoopA
    },
    miniGameStartDelay = 0.01,
    miniGame = {
      nTimeOut = 35,
      bExtraHudParameters = true,
      nTranslucency = 255,
      bShowTimer = false,
      nXPosition = 0.5,
      nYPosition = 0.2,
      sAction = "tap",
      button = Controller.RPad_Right,
      nDriverDifficulty = 1,
      nSuccessThreshold = 1
    },
    OnFailureAnimationBegin = OnFailureEvents,
    tFailureMultiEvents = {
      {
        nTime = 0.6,
        tControllerRumble = {nlength = 0.5}
      }
    }
  }
  self[6] = {
    hijackerAnimation = "ActionHijackHijackerE",
    hijackeeAnimation = "ActionHijackHijackeeE",
    hijackerAnimationFail = "ActionHijackHijackerFailE",
    hijackeeAnimationFail = "ActionHijackHijackeeFailE",
    vehicleAnimation = door_anim_E,
    vehicleAnimationFail = door_anim_EFail,
    tCharactersFaceStates = {
      hijackee = {
        faceStateA = {state = "Scared", weight = 1}
      }
    },
    OnFailureAnimationBegin = SetupGrenadeExplosion,
    miniGameStartDelay = 0.3,
    miniGame = {
      nTimeOut = 0.73,
      bExtraHudParameters = true,
      nTranslucency = 255,
      nXPosition = 0,
      nYPosition = 0.8,
      bShowTimer = false,
      sAction = "press",
      button = Controller.RPad_Down,
      nHudButtonMotionSpeed = 0.2
    },
    tMultiEvents = {
      {
        nTime = 0.1,
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
      nDelay = 1.8333,
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

function OnActionHijackComplete(self)
  local bResult
  bResult = Object.Detach(self._vehicle, uGunInstance)
  Debug.Printf("M1A2: RemoveWeapons: detach gun: " .. tostring(bResult))
  bResult = Object.Remove(uGunInstance)
  Debug.Printf("M1A2: RemoveWeapons: remove gun: " .. tostring(bResult))
  bResult = Object.Detach(self._vehicle, uGrenadeInstance)
  Debug.Printf("M1A2: RemoveWeapons: detach grenade: " .. tostring(bResult))
  bResult = Object.Remove(uGrenadeInstance)
  Debug.Printf("M1A2: RemoveWeapons: remove grenade: " .. tostring(bResult))
end

function OnFailureAnimationBegin(self, nCurrent)
  Debug.Printf("OnFailureAnimationBegin: nCurrent: " .. tostring(nCurrent))
  if nCurrent == 3 then
    Debug.Printf("OnFailureAnimationBegin: queue gunshot VFX")
    Event.Create(Event.TimerRelative, {0.2333}, function(self)
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

function SetupGrenadeExplosion(self, nCurrent)
  Debug.Printf("SetupGrenadeExplosion: ----")
  local event = Event.Create(Event.TimerRelative, {1.44}, OnStartGrenadeExplosion, {self})
  Debug.Printf("SetupGrenadeExplosion: ____")
end

function OnStartGrenadeExplosion(self)
  Debug.Printf("OnStartGrenadeExplosion")
  local x, y, z = Object.GetHardpointPosition(self._vehicle, "hp_actionhijack_grenadeexplosion")
  local player = Player.GetLocalPlayer()
  local playerCamera = Player.GetCamera(player)
  local playerCharacter = Player.GetCharacter(player)
  if x and y and z then
    Debug.Printf("OnStartGrenadeExplosion: spawning!")
    Pg.Spawn("Explosion (Grenade)", x, y, z, 0)
    Camera.Shake(playerCamera, "ShakeCameraMedium", playerCharacter, 5, 0.5)
    Pg.Rumble(self._hijacker, 1)
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
