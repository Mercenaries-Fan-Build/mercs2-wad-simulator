import("MrxUtil")
import("MrxGui")
import("MrxGuiManager")
import("MrxGuiHudActionHijack")
import("MrxSound")
import("Hero")
import("MrxAchievements")
import("MrxMusic")
import("WifFreePlay")
local nVehicleAnimBlendTime = 0.2
local nLinkedObjectInstance
local tDifficulty = {
  EASYTAP = {
    1,
    1.2,
    1.4
  },
  MEDTAP = {
    0.8,
    1,
    1.2
  },
  HARDTAP = {
    0.6,
    0.8,
    1
  },
  EASYMASH = {
    1,
    1.2,
    1.4
  },
  MEDMASH = {
    1,
    1.2,
    1.4
  },
  HARDMASH = {
    1,
    1.2,
    1.4
  }
}
local tRagdoll = {
  nDURATION = 1,
  GRAPHIC = Controller.RPad_Down,
  INPUT = Controller.RPad_Down,
  nTimeReduction = 1,
  nTimeReduction2 = 1.5,
  nKnockdown2 = 0.5,
  nKnockdown3 = 0.5
}
_bIsInHijack = false
_fUnloadCallback = nil
_tUnloadCallbackArgs = nil

function IsInHijack()
  return _bIsInHijack
end

function SetUnloadCallback(fCallback, tCallbackArgs)
  _fUnloadCallback = fCallback
  _tUnloadCallbackArgs = tCallbackArgs
end

RULESET_TANK = 0
RULESET_HELICOPTER = 1
RULESET_APC = 2
RULESET_SOLANO = nil

function CheckGoodStart(uVehicleObject, nRuleSet)
  if nRuleSet == RULESET_HELICOPTER and Vehicle.IsFlying(uVehicleObject) then
    return true
  end
  Debug.Printf("CheckGoodStart: false")
  return false
end

function InitializeActionHijack(self)
  self.tAnimation = {}
  self.tEvent = {}
  MrxGuiManager.ToggleHud(self._hijackerPlayer, false, "hijack")
  Vehicle.EnableTurret(self._hijackee, "head", false, "all", false)
  if self._hijackerPlayer == Player.GetLocalPlayer() then
    MrxSound.BeginActionHijack(not RULESET_SOLANO)
  end
  Object.SetInvincible(self._hijackee, true, "Hijack")
  Object.SetInvincible(self._hijacker, true, "Hijack")
  Object.SetInvincible(self._vehicle, true, "Hijack")
  if Player.GetLocalCharacter() == self._hijacker then
    bNeedAtmosphereChange = true
    ChangeAtmosphere(true)
    Graphics.Camera.SetFocusParams(0, 0, 0.1, 4, 6, 1)
    Graphics.Effect.CameraFade(0)
  end
  self._bRemote = Vehicle.IsHijackRemote and Vehicle.IsHijackRemote(self._hijacker)
  Debug.Printf("InitializeActionHijack;: HijackIsRemote: " .. tostring(self._bRemote))
  if self.tFaceAnimSets then
    for sActor, vSet in pairs(self.tFaceAnimSets) do
      local sSet = self.tFaceAnimSets[sActor].ActorAnim
      if sSet then
        local uGuid = self.tFaceAnimSets[sActor].ActorGuid
        local bSuccess = Animation.BindFaceAnimSet(uGuid, sSet)
        VO.SetCinematicMode(true)
        self._bUsingCinematicMode = true
      end
    end
  end
  if Vehicle.HijackStart then
    Vehicle.HijackStart(self._hijacker, self._hijackee, self._vehicle, self)
  end
  self.bDidSuccess = false
  self.bDidFailure = false
  _bIsInHijack = true
  WifFreePlay.StopNag()
end

function TankPrep(self)
  self._bTankCleanup = true
  Ai.Enable(self._hijackee, false)
  Vehicle.ClearControls(self._vehicle)
  Object.DisablePhysics(self._hijacker)
  Vehicle.EnableTurret(self._vehicle, "main_turret", false, "pitch", false)
  Vehicle.SetTurretPitch(self._vehicle, "main_turret", 0)
  Object.SetVisible(self._hijackee, true)
end

function Begin(self, i)
  Debug.Printf("@@@@@@ START - BEGIN @@@@")
  Event.Post("ActionHijackStart", {self})
  self.nCurrent = i
  if Vehicle.SetHijackState and not self._bRemote then
    Vehicle.SetHijackState(self._hijacker, i)
  end
  if self[self.nCurrent].nReactiveLoop ~= nil then
    Debug.Printf("Begin: PlayReactiveLoop: " .. tostring(i))
    PlayReactiveLoop(self, self[self.nCurrent])
  else
    Debug.Printf("Begin: Play: " .. tostring(i))
    Play(self, self[self.nCurrent].hijackerAnimation, self[self.nCurrent].hijackeeAnimation, self[self.nCurrent].vehicleAnimation, self[self.nCurrent].ExtraActors, self[self.nCurrent].tFaceAnimations, self[self.nCurrent].tCharactersFaceStates)
    if self[self.nCurrent].nTankHijackExplosion then
      Debug.Printf("Begin: spawning tank hijack explosion")
      Event.Delete(self.tEvent._explosionTimer)
      self.tEvent._explosionTimer = Event.Create(Event.TimerRelative, {
        self[self.nCurrent].nTankHijackExplosion.nDelay,
        false
      }, function(self)
        Debug.Printf("Spawning in-tank explosion now")
        local x, y, z = Object.GetHardpointPosition(self._vehicle, "hp_seat_lt")
        if self[self.nCurrent].nTankHijackExplosion.fSetCameraShake ~= nil and Player.GetLocalCharacter() == self._hijacker then
          local fShakeTime = self[self.nCurrent].nTankHijackExplosion.fSetCameraShake
          local fShakeAmplitude = self[self.nCurrent].nTankHijackExplosion.fSetCameraAmplitude
          player = Player.GetLocalPlayer()
          playerCamera = Player.GetCamera(player)
          playerCharacter = Player.GetCharacter(player)
          Camera.Shake(playerCamera, "ShakeCameraMedium", playerCharacter, fShakeAmplitude, fShakeTime)
          Debug.Printf("CameraShake!!!!!!!")
        end
        if self[self.nCurrent].nTankHijackExplosion.fSetRumbleLength ~= nil then
          local fRumbleLength = self[self.nCurrent].nTankHijackExplosion.fSetRumbleLength
          Pg.Rumble(self._hijacker, fRumbleLength)
          Debug.Printf("Rumble!!!!!!!")
        end
        if x and y and z then
          Pg.Spawn("global_particle_explosion_tankhatch", x, y, z, 0)
        end
      end, {self})
    end
    if self[self.nCurrent].tMultiEvents ~= nil then
      local tMEvents = self[self.nCurrent].tMultiEvents
      local eCharGuid = self._hijacker
      _ProcessMultiEventTable(eCharGuid, tMEvents)
    end
    if self[self.nCurrent].nMultiEvent then
      Event.Create(Event.TimerRelative, {
        self[self.nCurrent].nChopperKill.nDelay,
        false
      }, function(self)
        Debug.Printf("@@@@@@@@ Chopper Kill ")
        local eLocation = Pg.GetGuidByName(self[self.nCurrent].nChopperKill.nLocation)
        x, y, z = Object.GetPosition(eLocation)
        local myHeli = Pg.Spawn(self[self.nCurrent].nChopperKill.dVehicle, x, y, z)
        Object.Kill(myHeli)
      end, {self})
    end
    if self[self.nCurrent].nControllerRumble then
      Event.Delete(self.tEvent._rumbleTimer)
      self.tEvent._rumbleTimer = Event.Create(Event.TimerRelative, {
        self[self.nCurrent].nControllerRumble.nDelay,
        false
      }, function(self)
        if Player.GetLocalCharacter() == self._hijacker then
          local fRumbleLength = self[self.nCurrent].nControllerRumble.fSetRumbleLength
          Pg.Rumble(self._hijacker, fRumbleLength)
        end
        if self[self.nCurrent].nControllerRumble.bPEvent ~= nil then
          local objectInstance = self[self.nCurrent].nControllerRumble.objectInstanceTemplate
          local objectHPBone = self[self.nCurrent].nControllerRumble.objectHardPointBoneName
          local particleName = self[self.nCurrent].nControllerRumble.sPFXname
          local d, f, g = Object.GetHardpointPosition(objectInstance, objectHPBone)
          Debug.Printf("bPEvent Begin: x, y, z: " .. tostring(d) .. ", " .. tostring(f) .. ", " .. tostring(g))
          if d and f and g then
            local efxSpawn = Pg.Spawn(particleName, d, f, g, 0)
            Object.SetTransformToObject(efxSpawn, objectInstance, objectHPBone)
            Debug.Printf("bPEvent Begin: spawned particle stuff")
          end
        end
      end, {self})
    end
    if self[self.nCurrent].nCameraParams then
      local nDelayTemp = self[self.nCurrent].nCameraParams.nDelay or 0
      Event.Delete(self.tEvent._cameraParamsTimer)
      self.tEvent._cameraParamsTimer = Event.Create(Event.TimerRelative, {nDelayTemp, false}, function(self)
        local nPlayerCam = self[self.nCurrent].nCameraParams.nPlayerCam or 0
        local nStartNear = self[self.nCurrent].nCameraParams.nStartNear or 0
        local nEndNear = self[self.nCurrent].nCameraParams.nEndNear or 0.1
        local nStartFar = self[self.nCurrent].nCameraParams.nStartFar or 4
        local nEndFar = self[self.nCurrent].nCameraParams.nEndFar or 6
        local nBlur = self[self.nCurrent].nCameraParams.nBlur or 0.2
        local nDuration = self[self.nCurrent].nCameraParams.nDuration or 0
        if Player.GetLocalCharacter() == self._hijacker then
          Graphics.Camera.SetFocusParams(nPlayerCam, nStartNear, nEndNear, nStartFar, nEndFar, nBlur, nDuration)
          Debug.Printf("@@@@@@@@@@CameraParams being set!!!!!!!")
        end
      end, {self})
    end
    if self[self.nCurrent].nAnimCameraShake then
      Debug.Printf("nAnimCameraShake is present no function to run yet")
    end
    if self[self.nCurrent].nChopperKill then
      Debug.Printf("@@@@@@@@ Chopper Kill ")
      Event.Create(Event.TimerRelative, {
        self[self.nCurrent].nChopperKill.nDelay,
        false
      }, function(self)
        local eLocation = Pg.GetGuidByName(self[self.nCurrent].nChopperKill.nLocation)
        x, y, z = Object.GetPosition(eLocation)
        local myHeli = Pg.Spawn(self[self.nCurrent].nChopperKill.dVehicle, x, y, z)
        Debug.Printf("@@@@@@@ myHeli = " .. tostring(myHeli))
        Object.Kill(myHeli)
        Debug.Printf("@@@@@@ KILL HELI!")
      end, {self})
    end
    if self._bRemote then
      self[self.nCurrent].OnAnimationComplete = OnAnimationCompleteRemote
    else
      self[self.nCurrent].OnAnimationComplete = OnAnimationComplete
    end
    Event.Delete(self.tEvent.eHumanActionComplete_Hijacker)
    self.tEvent.eHumanActionComplete_Hijacker = Event.Create(Event.HumanActionComplete, {
      self._hijacker
    }, self[self.nCurrent].OnAnimationComplete, {self})
    if self[self.nCurrent].bDriverDone == true or self[self.nCurrent].bDriverDoneRagdoll == true or self[self.nCurrent].bDriverDoneDead == true or self[self.nCurrent].bDriverDoneRemove == true or self[self.nCurrent].bDriverDoneStanding == true then
      Debug.Printf("Begin: setting up OnDriverDone")
      if Human.SetPreemptiveRagdoll ~= nil and (self[self.nCurrent].bDriverDoneRagdoll or self[self.nCurrent].bDriverDoneDead) then
        Debug.Printf("Failure ragdoll for Driver")
        Human.SetPreemptiveRagdoll(self._hijackee)
      end
      Event.Delete(self.tEvent.eHumanActionComplete_Driver)
      self.tEvent.eHumanActionComplete_Driver = Event.Create(Event.HumanActionComplete, {
        self._hijackee
      }, OnDriverDone, {self})
    end
    if self._ActorOne and self[self.nCurrent].bActorOneDoneDead == true then
      Debug.Printf("Begin: setting up OnDriverDone")
      if Human.SetPreemptiveRagdoll ~= nil and (self[self.nCurrent].bDriverDoneRagdoll or self[self.nCurrent].bActorOneDoneDead) then
        Debug.Printf("Failure ragdoll for Actor")
        Human.SetPreemptiveRagdoll(self._ActorOne)
      end
      Event.Delete(self.tEvent.eHumanActionComplete_ActorOne)
      self.tEvent.eHumanActionComplete_ActorOne = Event.Create(Event.HumanActionComplete, {
        self._ActorOne
      }, OnDriverDone, {self})
    end
    if self._ActorTwo and self[self.nCurrent].bActorTwoDoneDead == true then
      Debug.Printf("Begin: setting up OnDriverDone")
      if Human.SetPreemptiveRagdoll ~= nil and (self[self.nCurrent].bDriverDoneRagdoll or self[self.nCurrent].bActorTwoDoneDead) then
        Debug.Printf("Failure ragdoll for Actor")
        Human.SetPreemptiveRagdoll(self._ActorTwo)
      end
      Event.Delete(self.tEvent.eHumanActionComplete_ActorTwo)
      self.tEvent.eHumanActionComplete_ActorTwo = Event.Create(Event.HumanActionComplete, {
        self._ActorTwo
      }, OnDriverDone, {self})
    end
  end
  self[self.nCurrent]._buttonPressed = true
  self[self.nCurrent].bSuccess = true
  if self[self.nCurrent].miniGame and self._hijackerPlayer and not self._bRemote then
    self[self.nCurrent]._buttonPressed = false
    self[self.nCurrent].bSuccess = false
    self[self.nCurrent].OnMinigameStatus = OnMinigameStatus
    self[self.nCurrent].OnMinigameStart = OnMinigameStart
    Event.Delete(self.tEvent._buttonTimer)
    self.tEvent._buttonTimer = Event.Create(Event.TimerRelative, {
      self[self.nCurrent].miniGameStartDelay,
      false
    }, self[self.nCurrent].OnMinigameStart, {self})
  end
end

function _ProcessMultiEventTable(eCharGuid, tMultiTable)
  if type(tMultiTable) == "table" then
    for k, tEventData in pairs(tMultiTable) do
      tEventData._MultiEventTimer = Event.Create(Event.TimerRelative, {
        tEventData.nTime,
        false
      }, function(self)
        if type(tEventData.tControllerRumble) == "table" then
          local fRumbleLength = tEventData.tControllerRumble.nlength
          Pg.Rumble(eCharGuid, fRumbleLength)
          Debug.Printf("@@@@@@@@@@@EVENTRumble!!!@@@@@@@@@@@@" .. tostring(fRumbleLength))
        end
        if type(tEventData.tCameraShake) == "table" and Player.GetLocalCharacter() == eCharGuid then
          local fShakeTime = tEventData.tCameraShake.fSetCameraShake
          local fShakeAmplitude = tEventData.tCameraShake.fSetCameraAmplitude
          player = Player.GetLocalPlayer()
          playerCamera = Player.GetCamera(player)
          playerCharacter = Player.GetCharacter(player)
          Camera.Shake(playerCamera, "ShakeCameraMedium", playerCharacter, fShakeAmplitude, fShakeTime)
          Debug.Printf("@@@@@@@@@@@EVENTShake!!!@@@@@@@@@@@@" .. tostring(fShakeTime))
        end
        if type(tEventData.tCameraParams) == "table" then
          local nDelayTemp = tEventData.tCameraParams.nDelay or 0
          local nPlayerCam = tEventData.tCameraParams.nPlayerCam or 0
          local nStartNear = tEventData.tCameraParams.nStartNear or 0
          local nEndNear = tEventData.tCameraParams.nEndNear or 0.1
          local nStartFar = tEventData.tCameraParams.nStartFar or 10
          local nEndFar = tEventData.tCameraParams.nEndFar or 40
          local nBlur = tEventData.tCameraParams.nBlur or 0.2
          local nDuration = tEventData.tCameraParams.nDuration or 0
          local nAngle = tEventData.tCameraParams.nFoV or 55
          if Player.GetLocalCharacter() == eCharGuid then
            Graphics.Camera.SetFocusParams(nPlayerCam, nStartNear, nEndNear, nStartFar, nEndFar, nBlur, nDuration)
            Graphics.Camera.SetFovParams(nPlayerCam, nAngle, nDuration)
            Debug.Printf("@@@@@@@@@@@EVENTDoF!!!@@@@@@@@@@@@" .. tostring(nDuration))
            Debug.Printf("@@@@@@@@@@@EVENTFoV!!!@@@@@@@@@@@@" .. tostring(nAngle))
          end
        end
        if type(tEventData.tParticleEfx) == "table" then
          local bSetTransform = tEventData.tParticleEfx.bSetTransToObj or true
          local objectInstance = tEventData.tParticleEfx.objectInstanceTemplate
          Debug.Printf("@@@@@@@@@@@EVENTParticleEfx!!!@@@@@@@@@@@@" .. tostring(tEventData.objectInstanceTemplate))
          local objectHPBone = tEventData.tParticleEfx.objectHardPointBoneName
          Debug.Printf("@@@@@@@@ HPBONE: " .. tostring(objectHPBone))
          local particleName = tEventData.tParticleEfx.sPFXname
          Debug.Printf("@@@@@@@@ PARTICLE NAME: " .. tostring(particleName))
          local d, f, g = Object.GetHardpointPosition(objectInstance, objectHPBone)
          if d and f and g then
            local efxSpawn = Pg.Spawn(particleName, d, f, g, 0)
            if bSetTransform then
              Object.SetTransformToObject(efxSpawn, objectInstance, objectHPBone)
            end
            Debug.Printf("@@@@@@@@@@@EVENTParticleEfx!!!@@@@@@@@@@@@" .. tostring(efxSpawn))
          end
        end
        if type(tEventData.tDetachEvent) == "table" then
          local bResult, objectInstance, uRibbonInstance, uRibbonTemplate, particleName, objectHPBone
          objectInstance = tEventData.objectInstanceTemplate
          objectHPBone = tEventData.objectHardPointBoneName
          particleName = tEventData.sPFXname
          uRibbonTemplate = Pg.GetGuidByName(particleName)
          bResult, uRibbonInstance = Object.Attach(objectInstance, objectHPBone, uRibbonTemplate)
          local bResult
          bResult = Object.Detach(objectInstance, uRibbonInstance)
          bResult = Object.Remove(uRibbonInstance)
        end
        if type(tEventData.tPlaySingleVO) == "table" then
          local vSpeaker = tEventData.tPlaySingleVO.nSpeaker
          local sCueHandle = tostring(tEventData.tPlaySingleVO.nCueName)
          Debug.Printf("@@@@@@@@@@ Guid of dude playing VO: " .. tostring(vSpeaker) .. " -Cue name being played: " .. sCueHandle)
          VO.Cue(vSpeaker, sCueHandle, function()
            return true
          end, {}, VO.PRIORITY_CINEMATIC)
        end
        if type(tEventData.tPlayMusic) == "table" then
          local sMusic = tostring(tEventData.tPlayMusic.nMusicName)
          Debug.Printf("************************************************************************************************** MUSIC PLAY: " .. sMusic)
          MrxMusic.PlaySpecialMusic(sMusic)
        end
        if type(tEventData.tPlaySoundFx) == "table" then
          local sSoundFx = tostring(tEventData.tPlaySoundFx.nSoundFxName)
          Debug.Printf("************************************************************************************************** SOUNDFX PLAY: " .. sSoundFx)
          Sound.CueSound(0, sSoundFx)
        end
        if type(tEventData.tStopSoundFx) == "table" then
          local sSoundFx = tostring(tEventData.tStopSoundFx.nSoundFxName)
          Debug.Printf("************************************************************************************************** SOUNDFX STOP: " .. sSoundFx)
          Sound.StopSound(0, sSoundFx)
        end
        if type(tEventData.tHeliKill) == "table" then
          local ExplosionHP = tEventData.tHeliKill.tExplosionHP
          MrxUtil.SpawnObject("fx_Explosion_Huge", ExplosionHP)
          local uEffect = MrxUtil.SpawnObject("global_particle_firelargesmoke_infinite", ExplosionHP)
          Event.Create(Event.TimerRelative, {20}, Object.Remove, {uEffect})
        end
        if type(tEventData.tLinkObject) == "table" then
          local tData = tEventData.tLinkObject
          ChildObjectGuid = tData.vObject
          ParentObjectGuid = tData.vParent
          AttachPointName = tData.vAttachPoint
          StateValue = tData.vState
          ToggleLinkedObject(self, ChildObjectGuid, ParentObjectGuid, AttachPointName, StateValue)
        end
      end, {self})
    end
  end
end

function ToggleLinkedObject(self, ChildObject, ParentObject, AttachPoint, State)
  Debug.Printf("@@@@@------ The current value of nLinkedObjectInstance is: " .. tostring(nLinkedObjectInstance))
  ChildGuid = Pg.GetGuidByName(ChildObject)
  if State == true then
    bResult, nLinkedObjectInstance = Object.Attach(ParentObject, AttachPoint, ChildGuid)
    Debug.Printf("@@@@---- Object Attached!")
  elseif nLinkedObjectInstance then
    bResult = Object.Detach(ParentObject, nLinkedObjectInstance)
    Debug.Printf("@@@@---- Object Detached!")
    bResult = Object.Remove(nLinkedObjectInstance)
    Debug.Printf("@@@@---- Object Removed!")
  end
end

function PlayReactiveLoop(self)
  if type(self[self.nCurrent].tVehicleAnimations) == "table" then
    if type(self[self.nCurrent].tReactiveLoopFaceStates) == "table" then
      Play(self, self[self.nCurrent].tHijackerAnimations[self[self.nCurrent].nReactiveLoop], self[self.nCurrent].tHijackeeAnimations[self[self.nCurrent].nReactiveLoop], self[self.nCurrent].tVehicleAnimations[self[self.nCurrent].nReactiveLoop], nil, self[self.nCurrent].tReactiveLoopFaceStates[self[self.nCurrent].nReactiveLoop])
    else
      Play(self, self[self.nCurrent].tHijackerAnimations[self[self.nCurrent].nReactiveLoop], self[self.nCurrent].tHijackeeAnimations[self[self.nCurrent].nReactiveLoop], self[self.nCurrent].tVehicleAnimations[self[self.nCurrent].nReactiveLoop])
    end
  elseif type(self[self.nCurrent].tReactiveLoopFaceStates) == "table" then
    Play(self, self[self.nCurrent].tHijackerAnimations[self[self.nCurrent].nReactiveLoop], self[self.nCurrent].tHijackeeAnimations[self[self.nCurrent].nReactiveLoop], nil, nil, self[self.nCurrent].tReactiveLoopFaceStates[self[self.nCurrent].nReactiveLoop])
  else
    Play(self, self[self.nCurrent].tHijackerAnimations[self[self.nCurrent].nReactiveLoop], self[self.nCurrent].tHijackeeAnimations[self[self.nCurrent].nReactiveLoop])
  end
  CreateReactiveLoopAnimationCompleteEvent(self)
end

function CreateReactiveLoopAnimationCompleteEvent(self)
  Event.Delete(self.tEvent.eHumanActionComplete_Hijacker)
  self.tEvent.eHumanActionComplete_Hijacker = Event.Create(Event.HumanActionComplete, {
    self._hijacker
  }, OnReactiveLoopAnimationComplete, {self})
end

function OnReactiveLoopAnimationComplete(self)
  if self[self.nCurrent].bMinigameDone == true then
    return
  end
  CreateReactiveLoopAnimationCompleteEvent(self)
end

function Play(self, sHijackerAnimation, sHijackeeAnimation, sVehicleAnimation, sExtraActors, sReactiveLoopFaceStates, sCharactersFaceStates)
  Debug.Printf("Play:    hero: " .. tostring(sHijackerAnimation))
  Debug.Printf("Play:  driver: " .. tostring(sHijackeeAnimation))
  Debug.Printf("Play: vehicle: " .. tostring(sVehicleAnimation))
  if type(sHijackerAnimation) == "string" then
    Human.SetState(self._hijacker, "InVehicle", sHijackerAnimation)
  end
  if type(sHijackeeAnimation) == "string" then
    Human.SetState(self._hijackee, "InVehicle", sHijackeeAnimation)
  end
  if type(sVehicleAnimation) == "string" then
    Object.PlayAnimation(self._vehicle, sVehicleAnimation, false, "hijack", nVehicleAnimBlendTime, true)
  end
  if sExtraActors ~= nil then
    Debug.Printf("sExtraActors is not nil!")
    if self._ActorOne and type(sExtraActors.ActorOne) == "string" then
      Human.SetState(self._ActorOne, "InVehicle", sExtraActors.ActorOne)
    end
    if self._ActorTwo and type(sExtraActors.ActorTwo) == "string" then
      Human.SetState(self._ActorTwo, "InVehicle", sExtraActors.ActorTwo)
    end
  end
  if sReactiveLoopFaceStates ~= nil then
    Debug.Printf("sReactiveLoopFaceStates is not nil!")
    if type(sReactiveLoopFaceStates) == "table" then
      if type(sReactiveLoopFaceStates.hijacker) == "table" then
        charTable = sReactiveLoopFaceStates.hijacker
        charGuid = self._hijacker
        SetMultiFacialExpressions(charGuid, charTable)
        Debug.Printf("SetReactiveLoopFaceState:" .. tostring(charTable) .. ",PlayerGuid:" .. tostring(charGuid))
      end
      if type(sReactiveLoopFaceStates.hijackee) == "table" then
        charTable = sReactiveLoopFaceStates.hijackee
        charGuid = self._hijackee
        SetMultiFacialExpressions(charGuid, charTable)
        Debug.Printf("SetReactiveLoopFaceState:" .. tostring(charTable) .. ",HijackeeGuid:" .. tostring(charGuid))
      end
    else
      Debug.Printf("sReactiveLoopFaceStates is not a table")
    end
  end
  if sCharactersFaceStates ~= nil then
    Debug.Printf("sCharactersFaceStates is not nil!")
    if type(sCharactersFaceStates) == "table" then
      if type(sCharactersFaceStates.hijacker) == "table" then
        charTable = sCharactersFaceStates.hijacker
        charGuid = self._hijacker
        SetMultiFacialExpressions(charGuid, charTable)
        Debug.Printf("SetFaceState:" .. tostring(charTable) .. ",PlayerGuid:" .. tostring(charGuid))
      end
      if type(sCharactersFaceStates.hijackee) == "table" then
        charTable = sCharactersFaceStates.hijackee
        charGuid = self._hijackee
        SetMultiFacialExpressions(charGuid, charTable)
        Debug.Printf("SetFaceState:" .. tostring(charTable) .. ",HijackeeGuid:" .. tostring(charGuid))
      end
      if type(sCharactersFaceStates.actorOne) == "table" then
        charTable = sCharactersFaceStates.actorOne
        charGuid = self._ActorOne
        SetMultiFacialExpressions(charGuid, charTable)
        Debug.Printf("SetFaceState:" .. tostring(charTable) .. ",ActorOne:" .. tostring(charGuid))
      end
      if type(sCharactersFaceStates.actorTwo) == "table" then
        charTable = sCharactersFaceStates.actorTwo
        charGuid = self._ActorTwo
        SetMultiFacialExpressions(charGuid, charTable)
        Debug.Printf("SetFaceState:" .. tostring(charTable) .. ",ActorTwo:" .. tostring(charGuid))
      end
    else
      Debug.Printf("sCharactersFaceStates not a table")
    end
  end
end

function SetMultiFacialExpressions(sCharGuid, tCharTable)
  if type(tCharTable) == "table" then
    for sExprName, tExprData in pairs(tCharTable) do
      Debug.Printf("PlayFacialExpression:" .. "CharacterGuid:" .. tostring(sCharGuid) .. "," .. tostring(tExprData.state) .. "," .. tostring(tExprData.weight) .. "," .. tostring(tExprData.duration) .. "," .. tostring(tExprData.blend))
      Animation.PlayFacialExpression(sCharGuid, tExprData.state, tExprData.weight, tExprData.duration, tExprData.blend)
      Debug.Printf("PlayFacialExpressionComplete:")
    end
  else
    Debug.Printf("tCharTable is not a table")
  end
end

function OnMinigameStart(self)
  local nHudButtonMotionSpeed = -1
  local bShowSparks = false
  if self[self.nCurrent].miniGame.sAction == "press" then
    self[self.nCurrent].miniGame.nHudButtonMotionSpeed = self[self.nCurrent].miniGame.nHudButtonMotionSpeed or -1
    Event.Delete(self.tEvent._eMinigame)
    self.tEvent._eMinigame = Event.Create(Event.Minigame, {
      self._hijackerPlayer,
      self[self.nCurrent].miniGame.nTimeOut,
      self[self.nCurrent].miniGame.sAction,
      self[self.nCurrent].miniGame.button
    }, self[self.nCurrent].OnMinigameStatus, {self})
  elseif self[self.nCurrent].miniGame.sAction == "hold" then
    self[self.nCurrent].miniGame.nHudButtonMotionSpeed = self[self.nCurrent].miniGame.nHudButtonMotionSpeed or -1
    Event.Delete(self.tEvent._eMinigame)
    self.tEvent._eMinigame = Event.Create(Event.Minigame, {
      self._hijackerPlayer,
      self[self.nCurrent].miniGame.nTimeOut,
      self[self.nCurrent].miniGame.sAction,
      self[self.nCurrent].miniGame.button,
      self[self.nCurrent].miniGame.nTime
    }, self[self.nCurrent].OnMinigameStatus, {self})
  elseif self[self.nCurrent].miniGame.sAction == "tap" then
    self[self.nCurrent].miniGame.nHudButtonMotionSpeed = self[self.nCurrent].miniGame.nHudButtonMotionSpeed or 0.1
    self[self.nCurrent].miniGame.nPlayerScore = 0
    self[self.nCurrent].miniGame.nDriverScore = 0
    self[self.nCurrent].miniGame.nDriverDifficulty = self[self.nCurrent].miniGame.nDriverDifficulty or 0.1
    self[self.nCurrent].miniGame.nSuccessThreshold = self[self.nCurrent].miniGame.nSuccessThreshold or 3
    Event.Delete(self.tEvent._eMinigame)
    self.tEvent._eMinigame = Event.Create(Event.Minigame, {
      self._hijackerPlayer,
      self[self.nCurrent].miniGame.nTimeOut,
      self[self.nCurrent].miniGame.sAction,
      self[self.nCurrent].miniGame.button
    }, self[self.nCurrent].OnMinigameStatus, {self})
    self[self.nCurrent].OnDriverSimulatedButtonPress = OnDriverSimulatedButtonPress
    Event.Delete(self.tEvent._eventDriverSimulatedButton)
    self.tEvent._eventDriverSimulatedButton = Event.Create(Event.TimerRelative, {
      self[self.nCurrent].miniGame.nDriverDifficulty,
      false
    }, self[self.nCurrent].OnDriverSimulatedButtonPress, {self})
  elseif self[self.nCurrent].miniGame.sAction == "alternate" then
    self[self.nCurrent].miniGame.nHudButtonMotionSpeed = self[self.nCurrent].miniGame.nHudButtonMotionSpeed or 0.1
    self[self.nCurrent].miniGame.nPlayerScore = 0
    self[self.nCurrent].miniGame.nDriverScore = 0
    self[self.nCurrent].miniGame.nDriverDifficulty = self[self.nCurrent].miniGame.nDriverDifficulty or 0.1
    self[self.nCurrent].miniGame.nSuccessThreshold = self[self.nCurrent].miniGame.nSuccessThreshold or 3
    Event.Delete(self.tEvent._eMinigame)
    if self[self.nCurrent].miniGame.button == Controller.Use_Melee then
      self.tEvent._eMinigame = Event.Create(Event.Minigame, {
        self._hijackerPlayer,
        self[self.nCurrent].miniGame.nTimeOut,
        self[self.nCurrent].miniGame.sAction,
        Controller.RPad_Up,
        Controller.RPad_Right
      }, self[self.nCurrent].OnMinigameStatus, {self})
    elseif self[self.nCurrent].miniGame.button == Controller.Use_Reload then
      self.tEvent._eMinigame = Event.Create(Event.Minigame, {
        self._hijackerPlayer,
        self[self.nCurrent].miniGame.nTimeOut,
        self[self.nCurrent].miniGame.sAction,
        Controller.RPad_Up,
        Controller.RPad_Left
      }, self[self.nCurrent].OnMinigameStatus, {self})
    else
      self.tEvent._eMinigame = Event.Create(Event.Minigame, {
        self._hijackerPlayer,
        self[self.nCurrent].miniGame.nTimeOut,
        self[self.nCurrent].miniGame.sAction,
        Controller.LStick_Left,
        Controller.LStick_Right
      }, self[self.nCurrent].OnMinigameStatus, {self})
    end
    self[self.nCurrent].OnDriverSimulatedButtonPress = OnDriverSimulatedButtonPress
    Event.Delete(self.tEvent._eventDriverSimulatedButton)
    self.tEvent._eventDriverSimulatedButton = Event.Create(Event.TimerRelative, {
      self[self.nCurrent].miniGame.nDriverDifficulty,
      false
    }, self[self.nCurrent].OnDriverSimulatedButtonPress, {self})
  end
  if self[self.nCurrent].miniGame.sAction == "alternate" then
    print("---showbutton alternate")
    if self[self.nCurrent].miniGame.button == Use_Melee then
      MrxGuiHudActionHijack.ShowButton(self._hijackPlayer, self[self.nCurrent].miniGame.button, self[self.nCurrent].miniGame.nTimeOut, self[self.nCurrent].miniGame.nHudButtonMotionSpeed, self[self.nCurrent].miniGame.nXPosition, self[self.nCurrent].miniGame.nYPosition, self[self.nCurrent].miniGame.nTranslucency, self[self.nCurrent].miniGame.bShowSparks, self[self.nCurrent].miniGame.nElapsedTime, self[self.nCurrent].miniGame.bFillTimer, self[self.nCurrent].miniGame.bClockwise, self[self.nCurrent].miniGame.bIsRecovery, self[self.nCurrent].miniGame.bShowTimer, self[self.nCurrent].miniGame.nScale, bShowSparks)
    elseif self[self.nCurrent].miniGame.button == Use_Reload then
      MrxGuiHudActionHijack.ShowButton(self._hijackPlayer, self[self.nCurrent].miniGame.button, self[self.nCurrent].miniGame.nTimeOut, self[self.nCurrent].miniGame.nHudButtonMotionSpeed, self[self.nCurrent].miniGame.nXPosition, self[self.nCurrent].miniGame.nYPosition, self[self.nCurrent].miniGame.nTranslucency, self[self.nCurrent].miniGame.bShowSparks, self[self.nCurrent].miniGame.nElapsedTime, self[self.nCurrent].miniGame.bFillTimer, self[self.nCurrent].miniGame.bClockwise, self[self.nCurrent].miniGame.bIsRecovery, self[self.nCurrent].miniGame.bShowTimer, self[self.nCurrent].miniGame.nScale, bShowSparks)
    else
      MrxGuiHudActionHijack.ShowButton(self._hijackerPlayer, self[self.nCurrent].miniGame.button, self[self.nCurrent].miniGame.nTimeOut, self[self.nCurrent].miniGame.nHudButtonMotionSpeed, self[self.nCurrent].miniGame.nXPosition, self[self.nCurrent].miniGame.nYPosition, self[self.nCurrent].miniGame.nTranslucency, self[self.nCurrent].miniGame.bShowSparks, self[self.nCurrent].miniGame.nElapsedTime, self[self.nCurrent].miniGame.bFillTimer, self[self.nCurrent].miniGame.bClockwise, self[self.nCurrent].miniGame.bIsRecovery, self[self.nCurrent].miniGame.bShowTimer, self[self.nCurrent].miniGame.nScale, bShowSparks)
    end
    print("---showbutton alternate")
  elseif self[self.nCurrent].miniGame.bExtraHudParameters == true then
    Debug.Printf("---showbutton custom settings")
    MrxGuiHudActionHijack.ShowButton(self._hijackerPlayer, self[self.nCurrent].miniGame.button, self[self.nCurrent].miniGame.nTimeOut, self[self.nCurrent].miniGame.nHudButtonMotionSpeed, self[self.nCurrent].miniGame.nXPosition, self[self.nCurrent].miniGame.nYPosition, self[self.nCurrent].miniGame.nTranslucency, self[self.nCurrent].miniGame.bShowSparks, self[self.nCurrent].miniGame.nElapsedTime, self[self.nCurrent].miniGame.bFillTimer, self[self.nCurrent].miniGame.bClockwise, self[self.nCurrent].miniGame.bIsRecovery, self[self.nCurrent].miniGame.bShowTimer, self[self.nCurrent].miniGame.nScale)
    Debug.Printf("---showbutton custom settings")
  else
    Debug.Printf("---showbutton default settings")
    MrxGuiHudActionHijack.ShowButton(self._hijackerPlayer, self[self.nCurrent].miniGame.button, self[self.nCurrent].miniGame.nTimeOut, self[self.nCurrent].miniGame.nHudButtonMotionSpeed, nil, nil, nil, bShowSparks)
    Debug.Printf("---showbutton default settings")
  end
end

function OnMinigameStatus(self, sStatus, n)
  if sStatus ~= "update" then
  end
  if sStatus == "success" then
    Debug.Printf("OnMinigameStatus: success!")
    self[self.nCurrent].bSuccess = true
    MrxGuiHudActionHijack.HideButton(self._hijackerPlayer)
    Sound.CueSound(0, "ui_HUD_Minigame_Press_Button")
  elseif sStatus == "failed" then
    if self[self.nCurrent].nReactiveLoop ~= nil then
      MrxGuiHudActionHijack.ShowFail(self._hijackerPlayer)
      if self[self.nCurrent].nReactiveLoop > 1 then
        Debug.Printf("HandleTapMinigame: fail!")
        self[self.nCurrent].bSuccess = false
        self[self.nCurrent].bMinigameDone = true
        DoFailureAnimation(self)
        MrxGuiHudActionHijack.ShowFail(self._hijackerPlayer)
      else
        Debug.Printf("OnMinigameStatus: fail!")
        self[self.nCurrent].bSuccess = false
        MrxGuiHudActionHijack.ShowFail(self._hijackerPlayer)
      end
    else
      Debug.Printf("OnMinigameStatus: fail!")
      self[self.nCurrent].bSuccess = false
      MrxGuiHudActionHijack.ShowFail(self._hijackerPlayer)
    end
  elseif sStatus == "update" then
    if self[self.nCurrent].miniGame.sAction == "tap" then
      HandleTapMinigame(self, sStatus, n)
    elseif self[self.nCurrent].miniGame.sAction == "alternate" then
      HandleTapMinigame(self, sStatus, n)
    end
  elseif sStatus == "timeout" then
    Debug.Printf("OnMinigameStatus: timout!")
    self[self.nCurrent].bSuccess = false
    MrxGuiHudActionHijack.ShowFail(self._hijackerPlayer)
    self[self.nCurrent].bEndReactiveLoop = true
  else
    Debug.Printf("OnMinigameStatus: OH NOES! Bad status!! " .. tostring(sStatus))
    self[self.nCurrent].bSuccess = true
    MrxGuiHudActionHijack.HideButton(self._hijackerPlayer)
  end
end

function HandleTapMinigame(self, sStatus, n)
  if self[self.nCurrent].miniGame == nil or self[self.nCurrent].miniGame.nPlayerScore == nil or self[self.nCurrent].bMinigameDone == true or self[self.nCurrent].nReactiveLoop == nil then
    return
  end
  n = n or 1
  local bRetVal = true
  self[self.nCurrent].miniGame.nPlayerScore = self[self.nCurrent].miniGame.nPlayerScore + (1 - n)
  Debug.Printf("HandleTapMinigame: " .. tostring(sStatus) .. " score: " .. tostring(self[self.nCurrent].miniGame.nPlayerScore) .. " | " .. tostring(self[self.nCurrent].miniGame.nDriverScore) .. " | " .. tostring(self[self.nCurrent].miniGame.nSuccessThreshold))
  local bPlayerWon = false
  local bDriverWon = false
  if self[self.nCurrent].miniGame.nPlayerScore > self[self.nCurrent].miniGame.nSuccessThreshold then
    bPlayerWon = true
  end
  if sStatus == "driver" or self[self.nCurrent].miniGame.nDriverScore > self[self.nCurrent].miniGame.nSuccessThreshold then
    bDriverWon = true
  end
  if bPlayerWon or bDriverWon then
    Debug.Printf("HandleTapMinigame: round over")
    self[self.nCurrent].miniGame.nPlayerScore = 0
    self[self.nCurrent].miniGame.nDriverScore = 0
  end
  if bPlayerWon then
    Debug.Printf("HandleTapMinigame: Player won this section")
    self[self.nCurrent].nReactiveLoop = self[self.nCurrent].nReactiveLoop + 1
    bRetVal = false
    Sound.CueSound(0, "ui_HUD_Minigame_Press_Button")
    if self[self.nCurrent].nReactiveLoop > table.maxn(self[self.nCurrent].tHijackerAnimations) then
      Debug.Printf("HandleTapMinigame: success!")
      self[self.nCurrent].bSuccess = true
      self[self.nCurrent].bMinigameDone = true
      DeleteAllEvents(self)
      DoSuccessAnimation(self)
      Sound.CueSound(0, "ui_HUD_Minigame_Press_Button")
    else
      Debug.Printf("HandleTapMinigame: round PASS")
      PlayReactiveLoop(self)
    end
  end
  if bDriverWon or sStatus == "timeout" then
    Debug.Printf("HandleTapMinigame: Driver won this section")
    self[self.nCurrent].nReactiveLoop = self[self.nCurrent].nReactiveLoop - 1
    bRetVal = false
    Pg.Rumble(self._hijacker, 0.15555)
    if self[self.nCurrent].nReactiveLoop < 1 or sStatus == "timeout" then
      Debug.Printf("HandleTapMinigame: fail!")
      self[self.nCurrent].bSuccess = false
      self[self.nCurrent].bMinigameDone = true
      DeleteAllEvents(self)
      DoFailureAnimation(self)
      MrxGuiHudActionHijack.ShowFail(self._hijackerPlayer, 1)
    else
      Debug.Printf("HandleTapMinigame: round fail")
      self[self.nCurrent].miniGame.nDriverDifficulty = self[self.nCurrent].miniGame.nDriverDifficulty - self[self.nCurrent].miniGame.nDriverDifficulty * 0.1
      PlayReactiveLoop(self)
    end
  end
  return bRetVal
end

function OnDriverSimulatedButtonPress(self)
  if self[self.nCurrent].bEndReactiveLoop == true then
    DoFailureAnimation(self)
    return
  end
  Debug.Printf("Driver Simulated Button Press!")
  self[self.nCurrent].miniGame.nDriverScore = self[self.nCurrent].miniGame.nSuccessThreshold
  HandleTapMinigame(self, "driver")
  self.tEvent._eventDriverSimulatedButton = Event.Create(Event.TimerRelative, {
    self[self.nCurrent].miniGame.nDriverDifficulty,
    false
  }, self[self.nCurrent].OnDriverSimulatedButtonPress, {self})
end

function ActionHijackFinish(uHijacker, uHijackee, uVehicle, bSuccess)
  Debug.Printf("ActionHijackFinish: called " .. tostring(bSuccess) .. tostring(uHijacker) .. tostring(uHijackee) .. tostring(uVehicle))
  Event.Post("ActionHijackFinish", {
    Hijacker = uHijacker,
    Hijackee = uHijackee,
    Vehicle = uVehicle,
    Success = bSuccess
  })
  _bIsInHijack = false
  MrxUtil.CallWithOptionalArgs(_fUnloadCallback, _tUnloadCallbackArgs)
  SetUnloadCallback(nil, nil)
  WifFreePlay.StartNag()
end

function OnAnimationComplete(self)
  Debug.Printf("OnAnimationComplete: called " .. tostring(self[self.nCurrent].bSuccess))
  Event.Post("ActionHijackComplete", {self})
  if self[self.nCurrent].bSuccess == true then
    DoSuccessAnimation(self)
  else
    DoFailureAnimation(self)
  end
end

function DoSuccessAnimation(self)
  Debug.Printf("DoSuccessAnimation: SUCCESS")
  local i
  if type(self[self.nCurrent].GetNextSuccessAnimation) == "function" then
    i = self[self.nCurrent].GetNextSuccessAnimation(self, self.nCurrent)
    Debug.Printf("DoSuccessAnimation: CUSTOM next section is " .. tostring(i))
  else
    i = self.nCurrent + 1
  end
  DeleteAllEvents(self)
  if self[i] then
    MrxGuiHudActionHijack.HideButton(self._hijackerPlayer)
    self.nCurrent = i
    Debug.Printf("DoSuccessAnimation: next section is " .. tostring(self.nCurrent))
    Begin(self, self.nCurrent)
  else
    Debug.Printf("DoSuccessAnimation: Action Hijack Complete")
    if self._hijackerPlayer == Player.GetLocalPlayer() then
      MrxSound.EndActionHijack(not RULESET_SOLANO, true)
    end
    ActionHijackComplete(self)
  end
end

function DoFailureAnimation(self)
  Debug.Printf("DoFailureAnimation: FAILURE")
  if Vehicle.SetHijackSuccess and not self._bRemote then
    Vehicle.SetHijackSuccess(self._hijacker, false)
  end
  MrxGuiHudActionHijack.SetDisplayVisible(self._hijackerPlayer, false)
  DeleteAllEvents(self)
  if type(self[self.nCurrent].OnFailureAnimationBegin) == "function" then
    local bSkipFailAnimations = self[self.nCurrent].OnFailureAnimationBegin(self, self.nCurrent)
    if bSkipFailAnimations then
      Debug.Printf("DoFailureAnimation: Skipping failure animations for the failed section")
      return
    end
  end
  RestoreCamera(self)
  Debug.Printf("DoFailureAnimation: playing: " .. tostring(self[self.nCurrent].hijackerAnimationFail))
  if self[self.nCurrent].hijackerAnimationFail then
    Human.SetState(self._hijacker, "InVehicle", self[self.nCurrent].hijackerAnimationFail)
  end
  if self[self.nCurrent].tCharactersFaceStates ~= nil then
    if type(self[self.nCurrent].tCharactersFaceStates) == "table" then
      sCharacterTable = self[self.nCurrent].tCharactersFaceStates
      if type(sCharacterTable.hijackerFail) == "table" then
        charTable = sCharacterTable.hijackerFail
        charGuid = self._hijacker
        SetMultiFacialExpressions(charGuid, charTable)
        Debug.Printf("SetFailFaceState:" .. tostring(charTable) .. ",PlayerGuid:" .. tostring(charGuid))
      end
      if type(sCharacterTable.hijackeeFail) == "table" then
        charTable = sCharacterTable.hijackeeFail
        charGuid = self._hijackee
        SetMultiFacialExpressions(charGuid, charTable)
        Debug.Printf("SetFailFaceState:" .. tostring(charTable) .. ",HijackeeGuid:" .. tostring(charGuid))
      end
      if type(sCharacterTable.actorOneFail) == "table" then
        charTable = sCharacterTable.actorOneFail
        charGuid = self._ActorOne
        SetMultiFacialExpressions(charGuid, charTable)
        Debug.Printf("SetFailFaceState:" .. tostring(charTable) .. ",PlayerGuid:" .. tostring(charGuid))
      end
      if type(sCharacterTable.actorTwoFail) == "table" then
        charTable = sCharacterTable.actorTwoFail
        charGuid = self._ActorTwo
        SetMultiFacialExpressions(charGuid, charTable)
        Debug.Printf("SetFailFaceState:" .. tostring(charTable) .. ",PlayerGuid:" .. tostring(charGuid))
      end
    else
      Debug.Printf(tostring(self[self.nCurrent].tCharactersFaceStates))
      Debug.Printf("self[ self.nCurrent ].tCharactersFaceStates not a table failfaceshapes!!!")
    end
  else
    Debug.Printf("self[ self.nCurrent ].tCharactersFaceStates == nil")
  end
  if type(self[self.nCurrent].OnFailAnimationStart) == "function" then
    self[self.nCurrent].OnFailAnimationStart(self, self.nCurrent)
  end
  if self[self.nCurrent].hijackeeAnimationFail ~= nil then
    Debug.Printf("DoFailureAnimation: playing: " .. tostring(self[self.nCurrent].hijackeeAnimationFail))
    Human.SetState(self._hijackee, "InVehicle", self[self.nCurrent].hijackeeAnimationFail)
  end
  if self[self.nCurrent].vehicleAnimationFail ~= nil then
    Debug.Printf("DoFailureAnimation: playing: " .. tostring(self[self.nCurrent].vehicleAnimationFail))
    Object.PlayAnimation(self._vehicle, self[self.nCurrent].vehicleAnimationFail, false, "hijack", nVehicleAnimBlendTime, true)
  end
  Debug.Printf("ExtraActor DoFailureAnimation: Check " .. tostring(self[self.nCurrent].ExtraActors))
  if self[self.nCurrent].ExtraActors ~= nil then
    if self[self.nCurrent].ExtraActors.ActorOneAnimationFail ~= nil then
      Debug.Printf("DoFailureAnimation: playing ActorOne: " .. tostring(self[self.nCurrent].ExtraActors.ActorOneAnimationFail))
      Human.SetState(self._ActorOne, "InVehicle", self[self.nCurrent].ExtraActors.ActorOneAnimationFail)
    end
    if self[self.nCurrent].ExtraActors.ActorTwoAnimationFail ~= nil then
      Debug.Printf("DoFailureAnimation: playing ActorTwo: " .. tostring(self[self.nCurrent].ExtraActors.ActorTwoAnimationFail))
      Human.SetState(self._ActorTwo, "InVehicle", self[self.nCurrent].ExtraActors.ActorTwoAnimationFail)
    end
  end
  Debug.Printf("Failure ragdoll for Hero")
  if Human.SetPreemptiveRagdoll ~= nil then
    Human.SetPreemptiveRagdoll(self._hijacker)
  end
  self.tEvent._FailureAnimationTimer = Event.Create(Event.HumanActionComplete, {
    self._hijacker
  }, OnFailAnimationComplete, {self})
end

function OnFailAnimationComplete(self)
  uHijacker = self._hijacker
  uHijackee = self._hijackee
  uVehicle = self._vehicle
  bSuccess = self[self.nCurrent].bSuccess
  DeleteAllEvents(self)
  if type(self[self.nCurrent].OnFailureAnimationComplete) == "function" then
    if RULESET_SOLANO == true then
      CleanupCommonNonSuccess(self, true)
      MrxGuiManager.ToggleHud(self._hijackerPlayer, true)
      OnRagdollDone(self, bSuccess)
      Debug.Printf("@@@@@@@@@@@@@@@@@@@@@@@@ thOnRagDoll")
      Debug.Printf("Solano cleanup rules complete!!")
    end
    self[self.nCurrent].OnFailureAnimationComplete(self, self.nCurrent)
    Debug.Printf("OnFailAnimationComplete: using custom handler")
  else
    CleanupCommonNonSuccess(self, true)
    if not self._bRemote then
      Debug.Printf("OnFailAnimationComplete: Starting ragdoll minigame")
      self.tEvent._eMinigame = Event.Create(Event.TimerRelative, {nKnockdownDuration}, OnRagdollMinigameDone, {self, true})
      Debug.Printf("OnFailAnimationComplete: set camera")
      local uPlayer = Object.IsPlayerControlled(self._hijacker)
      local uCamera = Player.GetCamera(uPlayer)
      Player.SetCinematicMode(uPlayer, true, true)
      Camera.Blend(uCamera, 1)
      Camera.SetLookAt(uCamera, self._hijacker, "bone_chest")
      Camera.Hold(uCamera, true, false)
    else
      Debug.Printf("OnFailAnimationComplete: Starting remote ragdoll minigame")
      OnRagdollDone(self, bSuccess)
    end
    Debug.Printf("OnFailAnimationComplete: ----")
  end
  if self._hijackerPlayer == Player.GetLocalPlayer() then
    MrxSound.EndActionHijack(not RULESET_SOLANO, false)
  end
  ActionHijackFinish(uHijacker, uHijackee, uVehicle, bSuccess)
end

function OnRagdollMinigameUpdate(self, sStatus, n)
end

function OnRagdollMinigameDone(self, bSuccess)
  if bSuccess then
    MrxGuiHudActionHijack.HideButton(self._hijackerPlayer)
    MrxGuiManager.ToggleHud(self._hijackerPlayer, true)
  else
    MrxGuiHudActionHijack.ShowFail(self._hijackerPlayer)
    MrxGuiManager.ToggleHud(self._hijackerPlayer, true)
  end
  Event.Delete(self.tEvent._eMinigame)
  self.tEvent._eMinigame = nil
  OnRagdollDone(self, bSuccess)
end

function OnRagdollDone(self, bGetUp)
  Debug.Printf("OnRagdollDone: ----")
  Object.SetInvincible(self._hijacker, false, "Hijack")
  Object.SetInvincible(self._hijackee, false, "Hijack")
  Object.SetInvincible(self._vehicle, false, "Hijack")
  Player.SetCinematicMode(Object.IsPlayerControlled(self._hijacker), false)
  if self._bTankCleanup == true then
    Vehicle.StopTankHijackMotion(self._vehicle)
    Object.SetVisible(self._hijackee, false)
    Vehicle.EnableTurret(self._vehicle, "main_turret", true)
  end
  if type(self._OnActionHijackComplete) == "function" then
    Debug.Printf("OnRagdollDone: calling self._OnActionHijackComplete( self )")
    self._OnActionHijackComplete(self)
  end
  Debug.Printf("OnRagdollDone: ____")
  if Vehicle.HijackAbortDone then
    Vehicle.HijackAbortDone(self._hijacker)
  end
  Human.SetState(self._hijackee, "InVehicle", "Idle")
  Ai.Enable(self._hijackee, true)
  Debug.Printf("OnRagdollDone: ++++")
  self.tAnimation = {}
  self = nil
  Debug.Printf("OnRagdollDone: ====")
end

function ChangeAtmosphere(bBegin)
  bSafeToBegin = not Graphics.Atmosphere.IsInterpolating()
  if bSafeToBegin then
    if bBegin then
      Graphics.Atmosphere.Begin()
      vBloomTargetLuminance = Graphics.Atmosphere.GetValue("fBloomTargetLuminance")
      vBloomContrastLimit = Graphics.Atmosphere.GetValue("fBloomContastLimit")
      vBloomContrastMultiplier = Graphics.Atmosphere.GetValue("fBloomContastMultiplier")
      Debug.Printf("Starting ActionHijack Atmosphere effect")
      Graphics.Atmosphere.SetValue("fBloomContastLimit", 0.125)
      Graphics.Atmosphere.SetValue("fBloomContastMultiplier", 1.5)
      Graphics.Atmosphere.End(0.45)
      bNeedAtmosphereChange = false
    else
      Graphics.Atmosphere.Begin()
      Graphics.Atmosphere.SetValue("fBloomTargetLuminance", vBloomTargetLuminance)
      Graphics.Atmosphere.SetValue("fBloomContastLimit", vBloomContrastLimit)
      Graphics.Atmosphere.SetValue("fBloomContastMultiplier", vBloomContrastMultiplier)
      Graphics.Atmosphere.End(1.5)
      bNeedAtmosphereChange = false
    end
  elseif bBegin and bNeedAtmosphereChange then
    eventHandle = Event.Create(Event.TimerRelative, {1}, ChangeAtmosphere, {bBegin})
  end
  if not bBegin then
    bNeedAtmosphereChange = false
    if eventHandle then
      Event.Delete(eventHandle)
    end
    eventHandle = nil
    Debug.Printf("Ending ActionHijack Atmosphere effect")
  end
end

function RestoreCamera(self)
  if Player.GetLocalCharacter() == self._hijacker then
    ChangeAtmosphere(false)
    Graphics.Camera.RestoreFocusParams(0, 0.6)
    Graphics.Effect.CameraFade(1)
  end
end

function DisableFacialExpressions(self)
  Animation.PlayFacialExpression(self._hijackee, "DisableAll")
  Debug.Printf("DisableFacialExpressions: DisableAll for PlayFacialExpression hijackee")
  Animation.PlayFacialExpression(self._hijacker, "DisableAll")
  Debug.Printf("DisableFacialExpressions: DisableAll for PlayFacialExpression hijacker")
  if self._ActorOne ~= nil then
    Animation.PlayFacialExpression(self._ActorOne, "DisableAll")
    Debug.Printf("DisableFacialExpressions: DisableAll for PlayFacialExpression actorOne")
  end
  if self._ActorTwo ~= nil then
    Animation.PlayFacialExpression(self._ActorTwo, "DisableAll")
    Debug.Printf("DisableFacialExpressions: DisableAll for PlayFacialExpression actorTwo")
  end
  if self._bUsingCinematicMode then
    VO.SetCinematicMode(false)
    Debug.Printf("DisableFacialExpressions: Setting Vo Cinematic Mode to false")
  end
end

function CleanupCommonNonSuccess(self, bFail)
  if RULESET_SOLANO == true then
    Debug.Printf("RULESET_SOLANO == true")
  end
  DisableFacialExpressions(self)
  if RULESET_SOLANO == true then
    Human.SetState(self._hijackee, "InVehicle", "Idle")
  end
  if bFail == true then
    Debug.Printf("CleanupCommonNonSuccess: Going to ragdoll")
    nKnockdownDuration = tRagdoll.nKnockdown2
    if Hero.GetAttribute(self._hijacker, "Attitude") > 2 then
      nKnockdownDuration = tRagdoll.nKnockdown2
    end
    Human.Knockdown(self._hijacker, nKnockdownDuration)
  end
  Vehicle.HijackAbort(self._hijacker)
  Ai.Enable(self._hijackee, true)
  if RULESET_SOLANO == true then
    RULESET_SOLANO = nil
  end
end

function CompleteHijackNonFailure(self, bSuccess)
  Debug.Printf("CompleteHijackNonFailure: ----")
  uHijacker = self._hijacker
  uHijackee = self._hijackee
  uVehicle = self._vehicle
  Object.SetInvincible(self._hijacker, false, "Hijack")
  Object.SetInvincible(self._hijackee, false, "Hijack")
  Object.SetInvincible(self._vehicle, false, "Hijack")
  DisableFacialExpressions(self)
  RestoreCamera(self)
  MrxGuiHudActionHijack.HideButton(self._hijackerPlayer)
  MrxGuiManager.ToggleHud(self._hijackerPlayer, true)
  Vehicle.EnableTurret(self._hijackee, "head", true)
  if self._bTankCleanup == true then
    Vehicle.StopTankHijackMotion(self._vehicle)
    Vehicle.EnableTurret(self._vehicle, "main_turret", true)
  end
  if self._vehicle then
    Object.StopAnimationChannel(self._vehicle, "hijack")
  end
  if bSuccess == true then
    Vehicle.HijackComplete(self._hijacker)
    if type(self._OnActionHijackComplete) == "function" then
      Debug.Printf("ActionHijackComplete: calling self._OnActionHijackComplete( self )")
      self._OnActionHijackComplete(self)
    end
  end
  self = nil
  Debug.Printf("CompleteHijackNonFailure: ====")
  ActionHijackFinish(uHijacker, uHijackee, uVehicle, bSuccess)
end

function ActionHijackCancel(self)
  Debug.Printf("ActionHijackCancel: ----")
  if self._hijackerPlayer == Player.GetLocalPlayer() then
    MrxSound.EndActionHijack(not RULESET_SOLANO, false)
  end
  Player.SetCinematicMode(Object.IsPlayerControlled(self._hijacker), false)
  DeleteAllEvents(self)
  if RULESET_SOLANO == true then
    Debug.Printf("CompleteHijackNonFailure: Solano ruleset")
    if self.CancelContract then
      Debug.Printf("CompleteHijackNonFailure: Cancel contract")
      self.CancelContract()
    end
  end
  CleanupCommonNonSuccess(self, false)
  Human.SetState(self._hijacker, "Upright", "Idle")
  Object.EnablePhysics(self._hijacker)
  Vehicle.HijackAbortDone(self._hijacker)
  CompleteHijackNonFailure(self, false)
  Debug.Printf("ActionHijackCancel: ====")
end

function ActionHijackComplete(self)
  Debug.Printf("ActionHijackComplete: ----")
  if Player.GetLocalCharacter() == self._hijacker then
    MrxAchievements.AchievementAddCount("ACHIEVEMENT_HEAVY_METAL_THUNDER", 1, Player.GetLocalPlayer())
  end
  CompleteHijackNonFailure(self, self[self.nCurrent].bSuccess)
  Debug.Printf("ActionHijackComplete: ====")
end

function OnDriverDone(self)
  Debug.Printf("OnDriverDone: ----")
  local bRetVal
  Ai.Enable(self._hijackee, true)
  if self[self.nCurrent].bDriverDoneRagdoll then
    bRetVal = false
    if Human.ForceExitSeatNoSnap then
      bRetVal = true
      Human.ForceExitSeatNoSnap(self._hijackee)
    end
    Debug.Printf("OnDriverDone: hijackee exits vehicle: " .. tostring(bRetVal))
    if Hero.GetAttribute(self._hijacker, "Brawn") > 2 then
      Human.Knockdown(self._hijackee, tRagdoll.nKnockdown2)
    else
      Human.Knockdown(self._hijackee, tRagdoll.nKnockdown3)
    end
    Debug.Printf("OnDriverDone: hijackee enters ragdoll: " .. tostring(bRetVal))
  elseif self[self.nCurrent].bDriverDoneStanding then
    Debug.Printf("Driver done standing")
    bRetVal = false
    if Human.ForceExitSeatNoSnap then
      bRetVal = true
      Human.ForceExitSeatNoSnap(self._hijackee)
    end
    Debug.Printf("OnDriverDone: hijackee exits vehicle: " .. tostring(bRetVal))
  elseif self[self.nCurrent].bDriverDoneDead then
    if self[self.nCurrent].bDriverDoneDead ~= true then
      Debug.Printf("OnDriverDone: Unknown state for driver, defaulting to dead")
    end
    Debug.Printf("OnDriverDone: killing driver")
    if Human.ForceExitSeatNoSnap then
      Human.ForceExitSeatNoSnap(self._hijackee)
    end
    Object.Kill(self._hijackee)
  elseif self[self.nCurrent].bActorOneDoneDead then
    Debug.Printf("OnDriverDone: killing ActorOne")
    if Human.ForceExitSeatNoSnap then
      Human.ForceExitSeatNoSnap(self._ActorOne)
    end
    Object.Kill(self._ActorOne)
  elseif self[self.nCurrent].bActorTwoDoneDead then
    Debug.Printf("OnDriverDone: killing ActorTwo")
    if Human.ForceExitSeatNoSnap then
      Human.ForceExitSeatNoSnap(self._ActorTwo)
    end
    Object.Kill(self._ActorTwo)
  else
    if self[self.nCurrent].bDriverDoneRemove ~= true then
      Debug.Printf("OnDriverDone: Unknown state for driver, defaulting to remove-from-world")
    end
    Debug.Printf("OnDriverDone: Removing driver from world")
    Object.Remove(self._hijackee)
  end
  Debug.Printf("OnDriverDone: ====")
end

function DeleteAllEvents(self)
  Debug.Printf("DeleteAllEvents")
  for each, event in pairs(self.tEvent) do
    Event.Delete(event)
  end
  if self[self.nCurrent].tMultiEvents then
    local tMultiEvents = self[self.nCurrent].tMultiEvents
    for i, tEventData in ipairs(tMultiEvents) do
      if tEventData._MultiEventTimer then
        Event.Delete(tEventData._MultiEventTimer)
        tEventData._MultiEventTimer = nil
        Debug.Printf("DeleteAlltMultiEvents")
      end
    end
  end
end

function OnAnimationCompleteRemote(self)
  Debug.Printf("OnAnimationCompleteRemote")
end

function PushActionHijack(self, newState, bSuccess)
  Debug.Printf("PushActionHijack " .. tostring(newState) .. " " .. tostring(bSuccess))
  if newState == 0 then
    Debug.Printf("remote is done?")
    if bSuccess and not self.bDidSuccess then
      self.bDidSuccess = true
      DeleteAllEvents(self)
      ActionHijackComplete(self)
    end
    if not bSuccess and not self.bDidFailure then
      self.bDidFailure = true
      RestoreCamera(self)
      OnFailAnimationComplete(self)
    end
  elseif not bSuccess then
    if not self.bDidFailure then
      self.bDidFailure = true
      DoFailureAnimation(self)
    end
  elseif newState ~= self.nCurrent then
    DoSuccessAnimation(self)
  end
end
