inherit("MrxTaskContract")
import("MrxSubtitle")
import("MrxMultiPageMenu")
import("MrxUtil")
import("MrxLayerManager")
import("MrxShootingGallery")
import("MrxVoSequence")
import("MrxAchievements")
import("MrxMusic")
import("MrxState")
NETEVENT_SETSTARTUPWEAPONS = 0
NETEVENT_RETURNWEAPONS = 1
NETEVENT_TARGETSDOWN = 2
NETEVENT_TARGETSUP = 3
tLocalP2Weapons = nil
tP2Weapons = nil
evClientSetup = nil

function NetEventCallback(nEventId, tArgs)
  if nEventId == NETEVENT_SETSTARTUPWEAPONS then
    Debug.Printf("got NETEVENT_SETSTARTUPWEAPONS")
    if not tP2Weapons then
      if MrxState._GetTotalRefCount() == 0 then
        SetP2Weapons()
      else
        evClientSetup = Event.Create(Event.TimerRelative, {1}, NetEventCallback, {nEventId})
      end
    end
  elseif nEventId == NETEVENT_RETURNWEAPONS then
    Debug.Printf("got NETEVENT_RETURNWEAPONS")
    Event.Delete(evClientSetup)
    evClientSetup = nil
    if tP2Weapons then
      uCharacter = Player.GetSecondaryCharacter()
      Event.Delete(_evMoveWeapons)
      Object.SetInfiniteAmmo(uCharacter, false)
      for i, uWeapon in ipairs(tP2Weapons) do
        Debug.Printf("Printing tArgs" .. tostring(uWeapon))
      end
      Human.Inventory.SetAllWeapons(uCharacter, tP2Weapons)
      MrxUtil.EnableHeroWeapons(false)
      tP2Weapons = nil
    end
  elseif nEventId == NETEVENT_TARGETSDOWN then
    tTargetsToPop = {
      "_pmcoutpost_shootinggallerytarget01 0x0012d3ae",
      "_pmcoutpost_shootinggallerytarget01 0x0012d3af",
      "_pmcoutpost_shootinggallerytarget01 0x0012d3b0",
      "_pmcoutpost_shootinggallerytarget01 0x0012d3b1"
    }
    for i, sPainting in ipairs(tTargetsToPop) do
      Event.Create(Event.ObjectHibernation, {
        Pg.GetGuidByName(sPainting),
        "awake"
      }, Vehicle.OpenDoor, {
        Pg.GetGuidByName(sPainting),
        "pivot"
      })
    end
  elseif nEventId == NETEVENT_TARGETSUP then
    tTargetsToPop = {
      "_pmcoutpost_shootinggallerytarget01 0x0012d3ae",
      "_pmcoutpost_shootinggallerytarget01 0x0012d3af",
      "_pmcoutpost_shootinggallerytarget01 0x0012d3b0",
      "_pmcoutpost_shootinggallerytarget01 0x0012d3b1"
    }
    for i, sPainting in ipairs(tTargetsToPop) do
      Vehicle.CloseDoor(Pg.GetGuidByName(sPainting), "pivot")
    end
  end
end

function LoadAssets(self, tSaveData)
  tLayersToAdd = {
    "Vz_State_PmcCon033",
    "Vz_State_PmcCon033_PopDown"
  }
  MrxLayerManager.Add(tLayersToAdd, self.AssetsLoaded, {self})
  _SetupP1Weapons(self)
  Net.SendCustomEvent("PmcCon033", NETEVENT_SETSTARTUPWEAPONS, {}, true)
  Hud.SupportMenu:SetShootingGalleryMode({bEnable = true})
end

function Activated(self)
  MrxTaskContract.Activated(self)
  local nCompletions = self:GetNumCompletions()
  if nCompletions == 0 then
    nTimeLimit = 90
    sTimeToBeat = "1:30"
  elseif nCompletions == 1 then
    nTimeLimit = 60
    sTimeToBeat = "1:00"
  elseif 2 <= nCompletions then
    nTimeLimit = 45
    sTimeToBeat = "0:45"
  end
  self.CourseTimer = MrxTimer:Create({
    nStartTime = 0,
    nStopTime = 600,
    nStep = 0.1,
    iTray = 2,
    nWarning = nTimeLimit,
    bUseTenths = true,
    tDoneCallbacks = {
      {
        self._SetCancelMessage,
        {
          self,
          "[PmcCon032.Terms.CancelTime]"
        }
      },
      {
        self.Cancel,
        {self}
      }
    }
  })
  self:_CreateEvent(Event.TimerRelative, {10}, function()
    self:_CreateEvent(Event.Boundary, {
      Player.GetAllCharacters(),
      Pg.GetGuidByName("LR_PMCOOB"),
      "exit"
    }, function()
      self:_SetCancelMessage("[PmcCon031.OtherThing]")
      self:Cancel()
    end)
  end)
  Hud.ObjectiveTray:SetSlotToText({
    nSlot = 1,
    sText = "[PmcCon032.Terms.TimeToBeatText]" .. " " .. sTimeToBeat
  })
  _CountDownVOSetup(self, nTimeLimit)
  PlayMusic(self, nTimeLimit)
  _evClientJoinedPMC033 = Event.CreatePersistent(Event.ScriptEvent, {
    "mpPlayerJoin",
    function(tData)
      return Net.IsServer() and not Player.IsLocal(tData[1])
    end
  }, Net.SendCustomEvent, {
    "PmcCon033",
    NETEVENT_SETSTARTUPWEAPONS,
    {},
    true
  })
  if Player.GetSecondaryCharacter() then
    tLocalP2Weapons = Human.Inventory.GetAllWeapons(Player.GetSecondaryCharacter())
  end
  self:_SetupObjective()
end

function _SetupObjective(self)
  tTargetsToPop = {
    "_pmcoutpost_shootinggallerytarget01 0x0012d3ae",
    "_pmcoutpost_shootinggallerytarget01 0x0012d3af",
    "_pmcoutpost_shootinggallerytarget01 0x0012d3b0",
    "_pmcoutpost_shootinggallerytarget01 0x0012d3b1"
  }
  for i, sPainting in ipairs(tTargetsToPop) do
    self:_CreateEvent(Event.ObjectHibernation, {
      Pg.GetGuidByName(sPainting),
      "awake"
    }, Vehicle.OpenDoor, {
      Pg.GetGuidByName(sPainting),
      "pivot"
    })
  end
  Net.SendCustomEvent("PmcCon033", NETEVENT_TARGETSDOWN, {}, true)
  PointDist = 2.3
  if self:GetNumCompletions() <= 1 then
    MrxVoSequence.Start({
      "Fiona-In-Mission-MinorContract-Pmc33-03"
    })
  end
  self.CourseTimer:Start()
  MrxShootingGallery.SetupBorder(Pg.GetGuidByName("PMCCon033_LR1"))
  self:CreateChild({
    sName = "MoveToPoint",
    sDspShortDesc = "[PmcCon032.Objectives.MoveToSandbags]",
    sModuleName = "MrxTaskObjectiveDeliver",
    vTgtInclude = Player.GetAnyCharacter(),
    bDspBlp = true,
    vDestLoc = "PistolRange 0x0012d20a",
    fDist = PointDist,
    bStop = false,
    bUseDestRing = true,
    fOnComplete = function()
      _DestroyObj1(self)
    end
  })
end

function _DestroyObj1(self)
  Counter = 0
  tTargets = {
    "_pmcoutpost_shootinggallerytarget01 0x0012d202",
    "_pmcoutpost_shootinggallerytarget01 0x0012d201",
    "_pmcoutpost_shootinggallerytarget01 0x0012d203"
  }
  DestroyObj1 = self:CreateChild({
    sName = "RPG Range1",
    sModuleName = "MrxTaskObjectiveDestroy",
    vTgtInclude = tTargets,
    sDspShortDesc = "[PmcCon033.Objectives.Portaits]",
    tOnComplete = {
      {
        _MoveToPoint2,
        {self}
      }
    }
  })
  if not Object.IsAlive(Pg.GetGuidByName("_pmcoutpost_shootinggallerytarget01 0x0012d202")) and not Object.IsAlive(Pg.GetGuidByName("_pmcoutpost_shootinggallerytarget01 0x0012d201")) and not Object.IsAlive(Pg.GetGuidByName("_pmcoutpost_shootinggallerytarget01 0x0012d203")) then
    DestroyObj1:Complete()
  end
  for i, sTarget in ipairs(tTargets) do
    self:_CreateEvent(Event.ObjectDeath, {
      Pg.GetGuidByName(sTarget)
    }, function()
      Counter = Counter + 1
      self:_CreateEvent(Event.TimerRelative, {2}, function()
        Counter = Counter - 1
      end)
      if Counter == 3 then
        Counter = 0
        tVoTable = {
          "Fiona.Cam.02",
          "Fiona.xfio168",
          "Fiona-In-Mission-Contract-Chi02-33",
          "Fiona.va3fio12",
          "Fiona-In-Mission-MinorContract-Pmc33-01"
        }
        MrxVoSequence.Start({
          MrxUtil.GetRandomTableElement(tVoTable)
        })
      end
    end)
  end
end

function _MoveToPoint2(self)
  MrxShootingGallery.SetupBorder(Pg.GetGuidByName("PMCCon033_LR2"))
  self:CreateChild({
    sName = "MoveToPoint",
    sDspShortDesc = "[PmcCon032.Objectives.MoveToSandbags]",
    sModuleName = "MrxTaskObjectiveDeliver",
    vTgtInclude = Player.GetAnyCharacter(),
    bDspBlp = true,
    vDestLoc = "PistolRange 0x0012d20c",
    fDist = PointDist,
    bStop = false,
    bUseDestRing = true,
    fOnComplete = function()
      _DestroyObj2(self)
    end
  })
end

function _DestroyObj2(self)
  tTargets = {
    "_pmcoutpost_shootinggallerytarget01 0x0012d213",
    "_pmcoutpost_shootinggallerytarget01 0x0012d214",
    "_pmcoutpost_shootinggallerytarget01 0x0012d215"
  }
  DestroyObj2 = self:CreateChild({
    sName = "RPG Range2",
    sModuleName = "MrxTaskObjectiveDestroy",
    vTgtInclude = tTargets,
    sDspShortDesc = "[PmcCon033.Objectives.Portaits]",
    tOnComplete = {
      {
        _MoveToPoint3,
        {self}
      }
    }
  })
  if not Object.IsAlive(Pg.GetGuidByName("_pmcoutpost_shootinggallerytarget01 0x0012d213")) and not Object.IsAlive(Pg.GetGuidByName("_pmcoutpost_shootinggallerytarget01 0x0012d214")) and not Object.IsAlive(Pg.GetGuidByName("_pmcoutpost_shootinggallerytarget01 0x0012d215")) then
    DestroyObj2:Complete()
  end
  for i, sTarget in ipairs(tTargets) do
    self:_CreateEvent(Event.ObjectDeath, {
      Pg.GetGuidByName(sTarget)
    }, function()
      Counter = Counter + 1
      self:_CreateEvent(Event.TimerRelative, {2}, function()
        Counter = Counter - 1
      end)
      if Counter == 3 then
        Counter = 0
        tVoTable = {
          "Fiona.Cam.02",
          "Fiona.xfio168",
          "Fiona-In-Mission-Contract-Chi02-33",
          "Fiona.va3fio12",
          "Fiona-In-Mission-MinorContract-Pmc33-01"
        }
        MrxVoSequence.Start({
          MrxUtil.GetRandomTableElement(tVoTable)
        })
      end
    end)
  end
end

function _MoveToPoint3(self)
  MrxShootingGallery.SetupBorder(Pg.GetGuidByName("PMCCon033_LR3"))
  self:CreateChild({
    sName = "MoveToPoint",
    sDspShortDesc = "[PmcCon032.Objectives.MoveToSandbags]",
    sModuleName = "MrxTaskObjectiveDeliver",
    vTgtInclude = Player.GetAnyCharacter(),
    bDspBlp = true,
    vDestLoc = "PistolRange 0x0012d217",
    fDist = PointDist,
    bStop = false,
    bUseDestRing = true,
    fOnComplete = function()
      _DestroyObj3(self)
    end
  })
end

function _DestroyObj3(self)
  tTargets = {
    "_pmcoutpost_shootinggallerytarget01 0x0012d218",
    "_pmcoutpost_shootinggallerytarget01 0x0012d219",
    "_pmcoutpost_shootinggallerytarget01 0x0012d21a"
  }
  DestroyObj3 = self:CreateChild({
    sName = "RPG Range3",
    sModuleName = "MrxTaskObjectiveDestroy",
    vTgtInclude = tTargets,
    sDspShortDesc = "[PmcCon033.Objectives.Portaits]",
    tOnComplete = {
      {
        _MoveToPoint4,
        {self}
      }
    }
  })
  if not Object.IsAlive(Pg.GetGuidByName("_pmcoutpost_shootinggallerytarget01 0x0012d218")) and not Object.IsAlive(Pg.GetGuidByName("_pmcoutpost_shootinggallerytarget01 0x0012d219")) and not Object.IsAlive(Pg.GetGuidByName("_pmcoutpost_shootinggallerytarget01 0x0012d21a")) then
    DestroyObj3:Complete()
  end
  for i, sTarget in ipairs(tTargets) do
    self:_CreateEvent(Event.ObjectDeath, {
      Pg.GetGuidByName(sTarget)
    }, function()
      Counter = Counter + 1
      self:_CreateEvent(Event.TimerRelative, {2}, function()
        Counter = Counter - 1
      end)
      if Counter == 3 then
        Counter = 0
        tVoTable = {
          "Fiona.Cam.02",
          "Fiona.xfio168",
          "Fiona-In-Mission-Contract-Chi02-33",
          "Fiona.va3fio12",
          "Fiona-In-Mission-MinorContract-Pmc33-01"
        }
        MrxVoSequence.Start({
          MrxUtil.GetRandomTableElement(tVoTable)
        })
      end
    end)
  end
end

function _MoveToPoint4(self)
  MrxShootingGallery.SetupBorder(Pg.GetGuidByName("PMCCon033_LR4"))
  self:CreateChild({
    sName = "MoveToPoint",
    sDspShortDesc = "[PmcCon032.Objectives.MoveToSandbags]",
    sModuleName = "MrxTaskObjectiveDeliver",
    vTgtInclude = Player.GetAnyCharacter(),
    bDspBlp = true,
    vDestLoc = "PistolRange 0x0012d226",
    fDist = PointDist,
    bStop = false,
    bUseDestRing = true,
    fOnComplete = function()
      _DestroyObj4(self)
    end
  })
end

function _DestroyObj4(self)
  tTargets = {
    "_pmcoutpost_shootinggallerytarget01 0x0012d22a",
    "_pmcoutpost_shootinggallerytarget01 0x0012d229",
    "_pmcoutpost_shootinggallerytarget01 0x0012d228"
  }
  DestroyObj4 = self:CreateChild({
    sName = "RPG Range4",
    sModuleName = "MrxTaskObjectiveDestroy",
    vTgtInclude = tTargets,
    sDspShortDesc = "[PmcCon033.Objectives.Portaits]",
    tOnComplete = {
      {
        _MoveToPoint5,
        {self}
      }
    }
  })
  if not Object.IsAlive(Pg.GetGuidByName("_pmcoutpost_shootinggallerytarget01 0x0012d22a")) and not Object.IsAlive(Pg.GetGuidByName("_pmcoutpost_shootinggallerytarget01 0x0012d229")) and not Object.IsAlive(Pg.GetGuidByName("_pmcoutpost_shootinggallerytarget01 0x0012d228")) then
    DestroyObj4:Complete()
  end
  for i, sTarget in ipairs(tTargets) do
    self:_CreateEvent(Event.ObjectDeath, {
      Pg.GetGuidByName(sTarget)
    }, function()
      Counter = Counter + 1
      self:_CreateEvent(Event.TimerRelative, {4}, function()
        Counter = Counter - 1
      end)
      if Counter == 3 then
        Counter = 0
        tVoTable = {
          "Fiona.Cam.02",
          "Fiona.xfio168",
          "Fiona-In-Mission-Contract-Chi02-33",
          "Fiona.va3fio12",
          "Fiona-In-Mission-MinorContract-Pmc33-01"
        }
        MrxVoSequence.Start({
          MrxUtil.GetRandomTableElement(tVoTable)
        })
      end
    end)
  end
end

function _MoveToPoint5(self)
  MrxShootingGallery.SetupBorder(Pg.GetGuidByName("PMCCon033_LR5"))
  self:CreateChild({
    sName = "MoveToPoint",
    sDspShortDesc = "[PmcCon032.Objectives.MoveToSandbags]",
    sModuleName = "MrxTaskObjectiveDeliver",
    vTgtInclude = Player.GetAnyCharacter(),
    bDspBlp = true,
    vDestLoc = "PistolRange 0x0012d3a7",
    fDist = PointDist,
    bStop = false,
    bUseDestRing = true,
    fOnComplete = function()
      _DestroyObj5(self)
    end
  })
end

function _DestroyObj5(self)
  DestroyObj5 = self:CreateChild({
    sName = "RPG Range5",
    sModuleName = "MrxTaskObjectiveDestroy",
    vTgtInclude = {
      "_pmcoutpost_shootinggallerytarget01 0x0012d3a8",
      "_pmcoutpost_shootinggallerytarget01 0x0012d3a9",
      "_pmcoutpost_shootinggallerytarget01 0x0012d3aa",
      "_pmcoutpost_shootinggallerytarget01 0x0012d3ab",
      "_pmcoutpost_shootinggallerytarget01 0x0012d3ac"
    },
    sDspShortDesc = "[PmcCon033.Objectives.Portaits]",
    tOnComplete = {
      {
        _DestroyObj6,
        {self}
      }
    }
  })
  if not Object.IsAlive(Pg.GetGuidByName("_pmcoutpost_shootinggallerytarget01 0x0012d3a8")) and not Object.IsAlive(Pg.GetGuidByName("_pmcoutpost_shootinggallerytarget01 0x0012d3a9")) and not Object.IsAlive(Pg.GetGuidByName("_pmcoutpost_shootinggallerytarget01 0x0012d3aa")) and not Object.IsAlive(Pg.GetGuidByName("_pmcoutpost_shootinggallerytarget01 0x0012d3ab")) and not Object.IsAlive(Pg.GetGuidByName("_pmcoutpost_shootinggallerytarget01 0x0012d3ac")) then
    DestroyObj5:Complete()
  end
end

function TempLoadLayers(self)
  MrxLayerManager.Remove({
    "Vz_State_PmcCon033_PopDown"
  })
  MrxLayerManager.Add({
    "Vz_State_PmcCon033_PopUp"
  }, _DestroyObj6, {self})
end

function _DestroyObj6(self)
  tTargetsToPop = {
    "_pmcoutpost_shootinggallerytarget01 0x0012d3ae",
    "_pmcoutpost_shootinggallerytarget01 0x0012d3af",
    "_pmcoutpost_shootinggallerytarget01 0x0012d3b0",
    "_pmcoutpost_shootinggallerytarget01 0x0012d3b1"
  }
  for i, sPainting in ipairs(tTargetsToPop) do
    Vehicle.CloseDoor(Pg.GetGuidByName(sPainting), "pivot")
  end
  Net.SendCustomEvent("PmcCon033", NETEVENT_TARGETSUP, {}, true)
  DestroyObj6 = self:CreateChild({
    sName = "RPG Range Pop",
    sModuleName = "MrxTaskObjectiveDestroy",
    vTgtInclude = tTargetsToPop,
    sDspShortDesc = "[PmcCon033.Objectives.Portaits]",
    fOnComplete = function()
      if MrxTimer.GetTime(self.CourseTimer) < nTimeLimit then
        CompleteVO(self)
      else
        self.CourseTimer:Pause()
        self._SetCancelMessage(self, "[PmcCon032.Terms.CancelTime]")
        self.Cancel(self)
      end
    end
  })
  if not Object.IsAlive(Pg.GetGuidByName("_pmcoutpost_shootinggallerytarget01 0x0012d3ae")) and not Object.IsAlive(Pg.GetGuidByName("_pmcoutpost_shootinggallerytarget01 0x0012d3af")) and not Object.IsAlive(Pg.GetGuidByName("_pmcoutpost_shootinggallerytarget01 0x0012d3b0")) and not Object.IsAlive(Pg.GetGuidByName("_pmcoutpost_shootinggallerytarget01 0x0012d3b1")) then
    DestroyObj6:Complete()
  end
end

function PlayMusic(self, nTimeLimit)
  MrxMusic.PlaySpecialMusic("mu_mission_pmccon033_01")
  Debug.Printf("Normal music start")
  SecondsTilSpeedUp = nTimeLimit - 25
  uMusicStartEvent = self:_CreateEvent(Event.TimerRelative, {SecondsTilSpeedUp}, function()
    Debug.Printf("Normal exciting start")
    MrxMusic.PlaySpecialMusic("mu_mission_pmccon033_02")
  end)
  uMusicEndEvent = self:_CreateEvent(Event.TimerRelative, {nTimeLimit}, function()
    Debug.Printf("Normal music start")
    MrxMusic.PlaySpecialMusic("mu_mission_pmccon033_01")
  end)
end

function _SetupP1Weapons(self)
  Debug.Printf("about inside _SetupP1Weapons")
  uCharacter = Player.GetLocalCharacter()
  tP1Weapons = Human.Inventory.GetAllWeapons(uCharacter)
  for i, uWeapon in ipairs(tP1Weapons) do
    Human.Inventory.DropWeapon(uCharacter, uWeapon)
  end
  _MoveWeapons(uCharacter, tP1Weapons)
  _evMoveWeapons = Event.CreatePersistent(Event.ObjectProximity, {
    tP1Weapons[1],
    uCharacter,
    ">",
    50
  }, _MoveWeapons, {uCharacter, tP1Weapons})
  Human.Inventory.SetAllWeapons(uCharacter, Pg.GetGuidByName("Pistol (silver)"))
  Object.SetInfiniteAmmo(uCharacter, true)
end

function SetP2Weapons()
  Debug.Printf("about inside SetP2Weapons")
  uCharacter = Player.GetSecondaryCharacter()
  tP2Weapons = Human.Inventory.GetAllWeapons(uCharacter)
  for i, uWeapon in ipairs(tP2Weapons) do
    Debug.Printf("Printing tArgs" .. tostring(uWeapon))
  end
  for i, uWeapon in ipairs(tP2Weapons) do
    Human.Inventory.DropWeapon(uCharacter, uWeapon)
  end
  _MoveWeapons(uCharacter, tP2Weapons)
  _evMoveWeapons = Event.CreatePersistent(Event.ObjectProximity, {
    tP2Weapons[1],
    uCharacter,
    ">",
    50
  }, _MoveWeapons, {uCharacter, tP2Weapons})
  Human.Inventory.SetAllWeapons(uCharacter, Pg.GetGuidByName("Pistol (silver)"))
  Object.SetInfiniteAmmo(uCharacter, true)
end

function _MoveWeapons(uCharacter, tWeapons)
  for i, uWeapon in ipairs(tWeapons) do
    x, y, z = Object.GetPosition(uCharacter)
    Object.DisablePhysics(uWeapon)
    Object.SetPosition(uWeapon, x, y - 5, z)
  end
end

function OnPlayerJoined(self, iPlayerId, uPlayerGuid, uCharGuid)
  self:_CreateEvent(Event.TimerRelative, {10}, function()
    if Player.GetSecondaryCharacter() then
      tLocalP2Weapons = Human.Inventory.GetAllWeapons(Player.GetSecondaryCharacter())
      self:_CreateEvent(Event.Boundary, {
        Player.GetSecondaryCharacter(),
        Pg.GetGuidByName("LR_PMCOOB"),
        "exit"
      }, function()
        self:_SetCancelMessage("[PmcCon031.OtherThing]")
        self:Cancel()
      end)
    end
  end)
end

function _CountDownVOSetup(self, nTimeLimit)
  uCountdownFail = self:_CreateEvent(Event.TimerRelative, {nTimeLimit}, FailureVO, {self})
  uCountdown5 = self:_CreateEvent(Event.TimerRelative, {
    nTimeLimit - 6.5
  }, MrxVoSequence.Start, {
    "Fiona-In-Mission-MinorContract-Pmc31-13"
  })
  uCountdown15 = self:_CreateEvent(Event.TimerRelative, {
    nTimeLimit - 16.5
  }, MrxVoSequence.Start, {
    "Fiona-In-Mission-MinorContract-Pmc31-12"
  })
  uCountdown30 = self:_CreateEvent(Event.TimerRelative, {
    nTimeLimit - 31.5
  }, MrxVoSequence.Start, {
    "Fiona-In-Mission-MinorContract-Pmc11-01"
  })
  tChrisNegativeVO = {
    "Chris.BadNews03",
    "Chris.Misc.Negative01",
    "Chris.Misc.Negative02",
    "Chris.Misc.Negative03",
    "Chris.Misc.Negative04"
  }
  tMattiasNegativeVO = {
    "Mattias.BadNews01",
    "Mattias.Misc.Negative05",
    "Mattias.Misc.Negative01",
    "Mattias.Misc.Negative01",
    "Mattias.Misc.Negative02",
    "Mattias.Misc.Negative03",
    "Mattias.Misc.Negative04",
    "Mattias.Misc.Negative05",
    "Mattias.BadNews03"
  }
  tJenNegativeVO = {
    "Jen.Negative01",
    "Jen.Negative02",
    "Jen.Negative05",
    "Jen.BadNews01"
  }
  uCountdownHero = self:_CreateEvent(Event.TimerRelative, {
    nTimeLimit - 3
  }, function()
    MrxVoSequence.Start({
      {
        mattias = MrxUtil.GetRandomTableElement(tMattiasNegativeVO),
        jennifer = MrxUtil.GetRandomTableElement(tJenNegativeVO),
        chris = MrxUtil.GetRandomTableElement(tChrisNegativeVO)
      }
    })
  end)
end

function CompleteVO(self)
  self.CourseTimer:Pause()
  MrxShootingGallery.SetupBorder(nil)
  if uCountdownFail then
    Event.Delete(uCountdownFail)
  end
  if uCountdown5 then
    Event.Delete(uCountdown5)
  end
  if uCountdown15 then
    Event.Delete(uCountdown15)
  end
  if uCountdown30 then
    Event.Delete(uCountdown30)
  end
  tPossibleVO = {
    "Fiona-In-Mission-MinorContract-Pmc31-02",
    "Fiona-In-Mission-MinorContract-Pmc31-20",
    "Fiona-In-Mission-MinorContract-Pmc31-21",
    "Fiona-In-Mission-MinorContract-Pmc32-01",
    "Fiona-In-Mission-MinorContract-Pmc32-02",
    "Fiona-In-Mission-MinorContract-Pmc34-01",
    "Fiona-In-Mission-MinorContract-Pmc31-33",
    "Fiona-In-Mission-MinorContract-Pmc31-35"
  }
  sVOLine = MrxUtil.GetRandomTableElement(tPossibleVO)
  MrxVoSequence.Start({
    sVOLine,
    {
      Complete,
      {self}
    }
  })
end

function FailureVO(self)
  tPossibleVO = {
    "Fiona-In-Mission-MinorContract-Pmc31-04",
    "Fiona-In-Mission-MinorContract-Pmc31-11",
    "Fiona-In-Mission-MinorContract-Pmc31-14",
    "Fiona-In-Mission-MinorContract-Pmc31-15",
    "Fiona-In-Mission-MinorContract-Pmc31-16",
    "Fiona-In-Mission-MinorContract-Pmc31-22",
    "Fiona-In-Mission-MinorContract-Pmc31-23",
    "Fiona-In-Mission-MinorContract-Pmc32-03",
    "Fiona-In-Mission-MinorContract-Pmc34-02"
  }
  if WifMissionFlow.HasKey("JetCon001") then
    table.insert(tPossibleVO, "Fiona-In-Mission-MinorContract-Pmc31-17")
  end
  sVOLine = MrxUtil.GetRandomTableElement(tPossibleVO)
  MrxVoSequence.Start({sVOLine})
end

function Complete(self)
  uCharacter = Player.GetLocalCharacter()
  Object.SetInfiniteAmmo(uCharacter, false)
  if uMusicStartEvent then
    Event.Delete(uMusicStartEvent)
  end
  Net.SendCustomEvent("PmcCon033", NETEVENT_RETURNWEAPONS, {}, true)
  Human.Inventory.SetAllWeapons(uCharacter, tP1Weapons)
  Human.DisableWeapons(uCharacter)
  if Player.GetSecondaryCharacter() then
    Human.DisableWeapons(Player.GetSecondaryCharacter())
  end
  MrxAchievements.NetGrantAchievement("ACHIEVEMENT_GONE_SHOOTIN")
  MrxTaskContract.Complete(self)
end

function Cancel(self)
  uCharacter = Player.GetLocalCharacter()
  Object.SetInfiniteAmmo(uCharacter, false)
  Net.SendCustomEvent("PmcCon033", NETEVENT_RETURNWEAPONS, {}, true)
  Human.Inventory.SetAllWeapons(uCharacter, tP1Weapons)
  Human.DisableWeapons(uCharacter)
  if Player.GetSecondaryCharacter() then
    Human.DisableWeapons(Player.GetSecondaryCharacter())
  end
  MrxTaskContract.Cancel(self)
end

function Cleanup(self)
  Event.Delete(_evClientJoinedPMC033)
  Event.Delete(_evMoveWeapons)
  Hud.SupportMenu:SetShootingGalleryMode({bEnable = false})
  Hud.ObjectiveTray:ClearSlot({nSlot = 1})
  Hud.ObjectiveTray:ClearSlot({nSlot = 2})
  MrxShootingGallery.SetupBorder(nil)
  MrxMusic.StopSpecialMusic()
  if self.CourseTimer then
    MrxTimer.Stop(self.CourseTimer)
  end
  for i, sLayerName in ipairs(tLayersToAdd) do
    MrxLayerManager.MarkForRemoval(sLayerName)
  end
  MrxTaskContract.Cleanup(self)
end
