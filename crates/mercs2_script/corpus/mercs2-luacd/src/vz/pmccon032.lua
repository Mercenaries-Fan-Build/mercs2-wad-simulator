inherit("MrxTaskContract")
import("MrxSubtitle")
import("MrxMultiPageMenu")
import("MrxUtil")
import("MrxLayerManager")
import("MrxShootingGallery")
import("MrxVoSequence")
import("MrxTimer")
import("MrxAchievements")
import("MrxMusic")
import("MrxState")
uWeapon = nil
WeaponClipAmmo = 0
WeaponReserveAmmo = 0
VOTimer = 1
NETEVENT_SETSTARTUPWEAPONS = 0
NETEVENT_RETURNWEAPONS = 1
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
  end
end

function LoadAssets(self, tSaveData)
  tLayersToAdd = {
    "Vz_State_PmcCon032"
  }
  Hud.SupportMenu:SetShootingGalleryMode({bEnable = true})
  MrxLayerManager.Add(tLayersToAdd, self.AssetsLoaded, {self})
  _SetupP1Weapons(self)
  if Player.GetSecondaryCharacter() then
    self:_CreateEvent(Event.ObjectHibernation, {
      Player.GetSecondaryCharacter(),
      "awake"
    }, function()
      Debug.Printf("attempting to give p2 correct weapons")
      Net.SendCustomEvent("PmcCon032", NETEVENT_SETSTARTUPWEAPONS, {}, true)
    end)
  end
end

function Activated(self)
  MrxTaskContract.Activated(self)
  nTimeLimit = 0
  local nCompletions = self:GetNumCompletions()
  if nCompletions == 0 then
    nTimeLimit = 150
    sTimeToBeat = "[PmcCon032.Terms.TimeToBeat3]"
  elseif nCompletions == 1 then
    nTimeLimit = 80
    sTimeToBeat = "[PmcCon032.Terms.TimeToBeat2]"
  elseif 2 <= nCompletions then
    nTimeLimit = 60
    sTimeToBeat = "[PmcCon032.Terms.TimeToBeat1]"
  end
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
  PlayMusic(self, nTimeLimit)
  _CountDownVOSetup(self, nTimeLimit)
  nHealth = Object.GetHealth(Player.GetLocalCharacter())
  self:_CreateEvent(Event.ObjectHealth, {
    Player.GetLocalCharacter(),
    "<",
    nHealth
  }, MrxVoSequence.Start, {
    {
      "Fiona-In-Mission-MinorContract-Pmc31-19"
    }
  })
  Hud.ObjectiveTray:SetSlotToText({
    nSlot = 1,
    sText = "[PmcCon032.Terms.TimeToBeatText] " .. sTimeToBeat
  })
  _evClientJoinedPMC032 = Event.CreatePersistent(Event.ScriptEvent, {
    "mpPlayerJoin",
    function(tData)
      return Net.IsServer() and not Player.IsLocal(tData[1])
    end
  }, Net.SendCustomEvent, {
    "PmcCon032",
    NETEVENT_SETSTARTUPWEAPONS,
    {},
    true
  })
  Net.SendCustomEvent("PmcCon032", NETEVENT_SETSTARTUPWEAPONS, {}, true)
  if Player.GetSecondaryCharacter() then
    tLocalP2Weapons = Human.Inventory.GetAllWeapons(Player.GetSecondaryCharacter())
  end
  self:_MoveToPoint1(nTimeLimit)
end

function _MoveToPoint1(self, nTimeLimit)
  Debug.Printf("About to setup timer!")
  self.CourseTimer = MrxTimer:Create({
    nStartTime = 0,
    nStopTime = 600,
    nStep = 0.1,
    nWarning = nTimeLimit,
    iTray = 2,
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
  Debug.Printf("About to start timer!")
  self.CourseTimer:Start()
  if self:GetNumCompletions() <= 1 then
    MrxVoSequence.Start({
      "Fiona-In-Mission-MinorContract-Pmc32-06"
    })
  end
  MrxShootingGallery.SetupBorder(Pg.GetGuidByName("PMCCon032_Easy_LR1"))
  self:CreateChild({
    sName = "MoveToPoint",
    sDspShortDesc = "[PmcCon032.Objectives.MoveToSandbags]",
    sModuleName = "MrxTaskObjectiveDeliver",
    vTgtInclude = Player.GetAnyCharacter(),
    bDspBlp = true,
    vDestLoc = "PmcCon032_Easy_Point1",
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
    "PMCCon032_Easy_Target1",
    "PMCCon032_Easy_Target2",
    "PMCCon032_Easy_Target3"
  }
  bAllDead = true
  for i, sTarget in ipairs(tTargets) do
    if Object.IsAlive(Pg.GetGuidByName(sTarget)) then
      bAllDead = false
    end
  end
  uCurObj = self:CreateChild({
    sName = "Gren Kill1",
    sModuleName = "MrxTaskObjectiveDestroy",
    vTgtInclude = tTargets,
    sDspShortDesc = "[PmcCon031.Objectives.TakeOut]",
    tOnComplete = {
      {
        _MoveToPoint2,
        {self}
      }
    }
  })
  if bAllDead == true then
    uCurObj.Complete(uCurObj)
  end
  for i, sTarget in ipairs(tTargets) do
    self:_CreateEvent(Event.ObjectDeath, {
      Pg.GetGuidByName(sTarget)
    }, function()
      Counter = Counter + 1
      self:_CreateEvent(Event.TimerRelative, {VOTimer}, function()
        Counter = Counter - 1
      end)
      if Counter == 2 then
        Counter = 0
        tVoTable = {
          "Fiona.Cam.02",
          "Fiona.xfio168",
          "Fiona-In-Mission-Contract-Chi02-33",
          "Fiona-None-Freeplay-None-14",
          "Fiona.xfio164",
          "Fiona.va3fio12"
        }
        MrxVoSequence.Start({
          MrxUtil.GetRandomTableElement(tVoTable)
        })
      end
    end)
  end
end

function _MoveToPoint2(self)
  MrxShootingGallery.SetupBorder(Pg.GetGuidByName("PMCCon032_Easy_LR2"))
  self:CreateChild({
    sName = "MoveToPoint",
    sDspShortDesc = "[PmcCon032.Objectives.MoveToSandbags]",
    sModuleName = "MrxTaskObjectiveDeliver",
    vTgtInclude = Player.GetAnyCharacter(),
    bDspBlp = true,
    vDestLoc = "PmcCon032_Easy_Point2",
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
    "PMCCon032_Easy_Target4",
    "PMCCon032_Easy_Target5",
    "PMCCon032_Easy_Target6"
  }
  bAllDead = true
  Counter = 0
  for i, sTarget in ipairs(tTargets) do
    if Object.IsAlive(Pg.GetGuidByName(sTarget)) then
      bAllDead = false
    end
  end
  uCurObj = self:CreateChild({
    sName = "Gren Kill2",
    sModuleName = "MrxTaskObjectiveDestroy",
    vTgtInclude = tTargets,
    sDspShortDesc = "[PmcCon032.Objectives.DestroyCars]",
    tOnComplete = {
      {
        _MoveToPoint3,
        {self}
      }
    }
  })
  if bAllDead == true then
    uCurObj.Complete(uCurObj)
  end
  for i, sTarget in ipairs(tTargets) do
    self:_CreateEvent(Event.ObjectDeath, {
      Pg.GetGuidByName(sTarget)
    }, function()
      Counter = Counter + 1
      self:_CreateEvent(Event.TimerRelative, {VOTimer}, function()
        Counter = Counter - 1
      end)
      if Counter == 2 then
        Counter = 0
        tVoTable = {
          "Fiona.Cam.02",
          "Fiona.xfio168",
          "Fiona-In-Mission-Contract-Chi02-33",
          "Fiona-None-Freeplay-None-14",
          "Fiona.xfio164",
          "Fiona.va3fio12"
        }
        MrxVoSequence.Start({
          MrxUtil.GetRandomTableElement(tVoTable)
        })
      end
    end)
  end
end

function _MoveToPoint3(self)
  MrxShootingGallery.SetupBorder(Pg.GetGuidByName("PMCCon032_Easy_LR3"))
  self:CreateChild({
    sName = "MoveToPoint",
    sDspShortDesc = "[PmcCon032.Objectives.MoveToSandbags]",
    sModuleName = "MrxTaskObjectiveDeliver",
    vTgtInclude = Player.GetAnyCharacter(),
    bDspBlp = true,
    vDestLoc = "PmcCon032_Easy_Point3",
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
    "PMCCon032_Easy_Target7",
    "PMCCon032_Easy_Target8",
    "PMCCon032_Easy_Target9"
  }
  bAllDead = true
  Counter = 0
  for i, sTarget in ipairs(tTargets) do
    if Object.IsAlive(Pg.GetGuidByName(sTarget)) then
      bAllDead = false
    end
  end
  uCurObj = self:CreateChild({
    sName = "Gren Kill3",
    sModuleName = "MrxTaskObjectiveDestroy",
    vTgtInclude = tTargets,
    sDspShortDesc = "[PmcCon031.Objectives.TakeOut]",
    tOnComplete = {
      {
        _MoveToPoint4,
        {self}
      }
    }
  })
  if bAllDead == true then
    uCurObj.Complete(uCurObj)
  end
  for i, sTarget in ipairs(tTargets) do
    self:_CreateEvent(Event.ObjectDeath, {
      Pg.GetGuidByName(sTarget)
    }, function()
      Counter = Counter + 1
      self:_CreateEvent(Event.TimerRelative, {VOTimer}, function()
        Counter = Counter - 1
      end)
      if Counter == 2 then
        Counter = 0
        tVoTable = {
          "Fiona.Cam.02",
          "Fiona.xfio168",
          "Fiona-In-Mission-Contract-Chi02-33",
          "Fiona-None-Freeplay-None-14",
          "Fiona.xfio164",
          "Fiona.va3fio12"
        }
        MrxVoSequence.Start({
          MrxUtil.GetRandomTableElement(tVoTable)
        })
      end
    end)
  end
end

function _MoveToPoint4(self)
  MrxShootingGallery.SetupBorder(Pg.GetGuidByName("PMCCon032_Easy_LR4"))
  self:CreateChild({
    sName = "MoveToPoint",
    sDspShortDesc = "[PmcCon032.Objectives.MoveToSandbags]",
    sModuleName = "MrxTaskObjectiveDeliver",
    vTgtInclude = Player.GetAnyCharacter(),
    bDspBlp = true,
    vDestLoc = "PmcCon032_Easy_Point4",
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
    "PMC011_SniperStatue18 0x0012d2d2",
    "PMC011_SniperStatue18 0x0012d2d3",
    "PMC011_SniperStatue18 0x0012d2d4",
    "PMC011_SniperStatue18 0x0012d2d5",
    "PMC011_SniperStatue18 0x0012d2d6",
    "PMC011_SniperStatue18 0x0012d2d7",
    "PMC011_SniperStatue18 0x0012d2d8",
    "PMC011_SniperStatue18 0x0012d2d9",
    "PMC011_SniperStatue18 0x0012d2da",
    "PMC011_SniperStatue18 0x0012d2db",
    "PMC011_SniperStatue18 0x0012d2dc",
    "PMC011_SniperStatue18 0x0012d2dd",
    "PMC011_SniperStatue18 0x0012d2de",
    "PMC011_SniperStatue18 0x0012d2df",
    "PMC011_SniperStatue18 0x0012d2e0",
    "PMC011_SniperStatue18 0x0012d2e1",
    "PMC011_SniperStatue18 0x0012d2e2",
    "PMC011_SniperStatue18 0x0012d2e3",
    "PMC011_SniperStatue18 0x0012d2e4",
    "PMC011_SniperStatue18 0x0012d2e5",
    "PMC011_SniperStatue18 0x0012d2e6"
  }
  bAllDead = true
  Counter = 0
  for i, sTarget in ipairs(tTargets) do
    if Object.IsAlive(Pg.GetGuidByName(sTarget)) then
      bAllDead = false
    end
  end
  uCurObj = self:CreateChild({
    sName = "Gren Kill4",
    sModuleName = "MrxTaskObjectiveDestroy",
    vTgtInclude = tTargets,
    sDspShortDesc = "[PmcCon031.Objectives.TakeOut]",
    tOnComplete = {
      {
        _MoveToPoint5,
        {self}
      }
    }
  })
  if bAllDead == true then
    uCurObj.Complete(uCurObj)
  end
  for i, sTarget in ipairs(tTargets) do
    self:_CreateEvent(Event.ObjectDeath, {
      Pg.GetGuidByName(sTarget)
    }, function()
      Counter = Counter + 1
      self:_CreateEvent(Event.TimerRelative, {VOTimer}, function()
        Counter = Counter - 1
      end)
      if Counter == 10 then
        Counter = 0
        tVoTable = {
          "Fiona.Cam.02",
          "Fiona.xfio168",
          "Fiona-In-Mission-Contract-Chi02-33",
          "Fiona-None-Freeplay-None-14",
          "Fiona.xfio164",
          "Fiona.va3fio12"
        }
        MrxVoSequence.Start({
          MrxUtil.GetRandomTableElement(tVoTable)
        })
      end
    end)
  end
end

function _MoveToPoint5(self)
  MrxShootingGallery.SetupBorder(Pg.GetGuidByName("PMCCon032_Easy_LR5"))
  self:CreateChild({
    sName = "MoveToPoint",
    sDspShortDesc = "[PmcCon032.Objectives.MoveToTower]",
    sModuleName = "MrxTaskObjectiveDeliver",
    vTgtInclude = Player.GetAnyCharacter(),
    bDspBlp = true,
    vDestLoc = "PmcCon032_Easy_Point5",
    fDist = PointDist,
    bStop = false,
    bUseDestRing = true,
    fOnComplete = function()
      _DestroyObj5(self)
    end
  })
end

function _DestroyObj5(self)
  Event.Delete(uCountdownHero)
  tTargets = {
    "PMCCon032_Easy_Target10",
    "PMCCon032_Easy_Target11",
    "PMCCon032_Easy_Target12",
    "PMCCon032_Easy_Target13",
    "PMCCon032_Easy_Target14",
    "PMCCon032_Easy_Target15"
  }
  bAllDead = true
  for i, sTarget in ipairs(tTargets) do
    if Object.IsAlive(Pg.GetGuidByName(sTarget)) then
      bAllDead = false
    end
  end
  oFinalObj = self:CreateChild({
    sName = "Gren Kill5",
    sModuleName = "MrxTaskObjectiveDestroy",
    vTgtInclude = tTargets,
    sDspShortDesc = "[PmcCon031.Objectives.TakeOut]",
    fOnComplete = function()
      if MrxTimer.GetTime(self.CourseTimer) < nTimeLimit then
        CompleteVO(self)
      else
        self.CourseTimer:Pause()
        self:_SetCancelMessage("[PmcCon032.Terms.CancelTime]")
        self.Cancel(self)
      end
    end
  })
  if bAllDead == true then
    oFinalObj.Complete(oFinalObj)
  end
end

function PlayMusic(self, nTimeLimit)
  MrxMusic.PlaySpecialMusic("mu_mission_pmccon032_01")
  SecondsTilSpeedUp = nTimeLimit - 25
  uMusicStartEvent = self:_CreateEvent(Event.TimerRelative, {SecondsTilSpeedUp}, function()
    MrxMusic.PlaySpecialMusic("mu_mission_pmccon032_02")
  end)
  uMusicEndEvent = self:_CreateEvent(Event.TimerRelative, {SecondsTilSpeedUp}, function()
    MrxMusic.PlaySpecialMusic("mu_mission_pmccon032_01")
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
  Human.Inventory.SetAllWeapons(uCharacter, Pg.GetGuidByName("Grenade Launcher"))
  Object.SetInfiniteAmmo(uCharacter, true)
  Debug.Printf("about done with _SetupP1Weapons")
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
  Human.Inventory.SetAllWeapons(uCharacter, Pg.GetGuidByName("Grenade Launcher"))
  Object.SetInfiniteAmmo(uCharacter, true)
end

function _MoveWeapons(uCharacter, tWeapons)
  for i, uWeapon in ipairs(tWeapons) do
    x, y, z = Object.GetPosition(uCharacter)
    Object.DisablePhysics(uWeapon)
    Object.SetPosition(uWeapon, x, y - 5, z)
  end
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
  if uMusicStartEvent then
    Event.Delete(uMusicStartEvent)
    uMusicStartEvent = self:_CreateEvent(Event.TimerRelative, {
      nTimeLimit - 25
    }, function()
      MrxMusic.PlaySpecialMusic("mu_mission_pmccon034_02")
      uMusicStartEvent = nil
    end)
  end
  if uMusicEndEvent then
    Event.Delete(uMusicEndEvent)
    uMusicEndEvent = self:_CreateEvent(Event.TimerRelative, {
      nTimeLimit - 25
    }, function()
      MrxMusic.PlaySpecialMusic("mu_mission_pmccon034_01")
      uMusicEndEvent = nil
    end)
  end
end

function CompleteVO(self)
  MrxShootingGallery.SetupBorder(nil)
  self.CourseTimer:Pause()
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
  Net.SendCustomEvent("PmcCon032", NETEVENT_RETURNWEAPONS, {}, true)
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
  Net.SendCustomEvent("PmcCon032", NETEVENT_RETURNWEAPONS, {}, true)
  Human.Inventory.SetAllWeapons(uCharacter, tP1Weapons)
  Human.DisableWeapons(uCharacter)
  if Player.GetSecondaryCharacter() then
    Human.DisableWeapons(Player.GetSecondaryCharacter())
  end
  MrxTaskContract.Cancel(self)
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

function Cleanup(self)
  Event.Delete(_evClientJoinedPMC032)
  Event.Delete(_evMoveWeapons)
  MrxShootingGallery.SetupBorder(nil)
  Hud.SupportMenu:SetShootingGalleryMode({bEnable = false})
  uCharacter = Player.GetSecondaryCharacter()
  if uCharacter then
    for i, uWeapon in ipairs(tLocalP2Weapons) do
      Debug.Printf(tostring(uWeapon))
    end
  end
  Hud.ObjectiveTray:ClearSlot({nSlot = 1})
  Hud.ObjectiveTray:ClearSlot({nSlot = 2})
  Hud.ObjectiveTray:ClearSlot({nSlot = 3})
  MrxLayerManager.MarkForAddition("vz_state_pmc")
  for i, sLayerName in ipairs(tLayersToAdd) do
    MrxLayerManager.MarkForRemoval(sLayerName)
  end
  if self.CourseTimer then
    self.CourseTimer:Stop()
  end
  MrxTaskContract.Cleanup(self)
end
