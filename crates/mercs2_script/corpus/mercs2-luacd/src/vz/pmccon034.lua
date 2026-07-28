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
    "vz_state_pmc",
    "Vz_State_PmcCon034"
  }
  Hud.SupportMenu:SetShootingGalleryMode({bEnable = true})
  MrxLayerManager.Add(tLayersToAdd, self.AssetsLoaded, {self})
  tLayersToAdd = {
    "Vz_State_PmcCon034"
  }
  _SetupP1Weapons(self)
  if Player.GetSecondaryCharacter() then
    self:_CreateEvent(Event.ObjectHibernation, {
      Player.GetSecondaryCharacter(),
      "awake"
    }, function()
      Debug.Printf("attempting to give p2 correct weapons")
      Net.SendCustomEvent("PmcCon034", NETEVENT_SETSTARTUPWEAPONS, {}, true)
    end)
  end
end

function Activated(self)
  MrxTaskContract.Activated(self)
  Hud.SupportMenu:SetShootingGalleryMode({bEnable = true})
  local nCompletions = self:GetNumCompletions()
  if nCompletions == 0 then
    nTimeLimit = 90
    TimeLimitDsp = "1:30"
  elseif nCompletions == 1 then
    nTimeLimit = 60
    TimeLimitDsp = "1:00"
  elseif 2 <= nCompletions then
    nTimeLimit = 30
    TimeLimitDsp = "0:30"
  end
  self:_CreateEvent(Event.TimerRelative, {10}, function()
    self:_CreateEvent(Event.Boundary, {
      Player.GetAnyCharacter(),
      Pg.GetGuidByName("LR_PMCOOB"),
      "exit"
    }, function()
      self:_SetCancelMessage("[PmcCon034.Terms.CancelOOB]")
      self:Cancel()
    end)
  end)
  PlayMusic(self, nTimeLimit)
  _CountDownVOSetup(self, nTimeLimit)
  _evClientJoinedPMC034 = Event.CreatePersistent(Event.ScriptEvent, {
    "mpPlayerJoin",
    function(tData)
      return Net.IsServer() and not Player.IsLocal(tData[1])
    end
  }, Net.SendCustomEvent, {
    "PmcCon034",
    NETEVENT_SETSTARTUPWEAPONS,
    {},
    true
  })
  Net.SendCustomEvent("PmcCon034", NETEVENT_SETSTARTUPWEAPONS, {}, true)
  self:_SetupObjective(self)
end

function _SetupObjective(self)
  Debug.Printf("about inside setupobjective")
  if Player.GetSecondaryCharacter() then
    tLocalP2Weapons = Human.Inventory.GetAllWeapons(Player.GetSecondaryCharacter())
  end
  iTruckTargetNum = 0
  _CallTruckTarget(self)
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
  self.CourseTimer:Start()
  if self:GetNumCompletions() <= 1 then
    MrxVoSequence.Start({
      "Fiona-In-Mission-MinorContract-Pmc34-03"
    })
  end
  Hud.ObjectiveTray:SetSlotToText({
    vPlayer = nil,
    nSlot = 1,
    sText = "[PmcCon032.Terms.TimeToBeatText] " .. TimeLimitDsp
  })
  Debug.Printf("about to start objective")
  self:CreateChild({
    sName = "DestroyStatues",
    sModuleName = "MrxTaskObjectiveDestroy",
    sDspShortDesc = "[PmcCon034.Objectives.DestroyStuff]",
    vTgtInclude = {
      "PMC011_SniperStatue_Easy1",
      "PMC011_SniperStatue_Easy2",
      "PMC011_SniperStatue_Easy3",
      "PMC011_SniperStatue_Easy4",
      "PMC011_SniperStatue_Easy5",
      "PMC011_SniperStatue_Easy6",
      "PMC011_SniperStatue_Easy7",
      "PMC011_SniperStatue_Easy8",
      "PMC011_SniperStatue_Easy9",
      "PMC011_SniperStatue_Easy10",
      "PMC011_SniperStatue_Easy11",
      "PMC011_SniperStatue_Easy12",
      "PMC011_SniperStatue_Easy13",
      "PMC011_SniperStatue_Easy14",
      "PMC011_SniperStatue_Easy15",
      "PMC011_SniperStatue_Easy16",
      "PMC011_SniperStatue_Easy17",
      "PMC011_SniperStatue_Easy18",
      "PMC011_SniperStatue_Easy19"
    },
    fOnComplete = function()
      EndTime = self.CourseTimer:GetTime()
      if EndTime > nTimeLimit then
        self:_SetCancelMessage("[PmcCon032.Terms.CancelTime]")
        self.CourseTimer:Pause()
        self.Cancel(self)
      else
        CompleteVO(self)
      end
    end
  })
  _AttachStatue(self)
end

function _SetupP1Weapons(self)
  Debug.Printf("about inside _SetupP1Weapons")
  uCharacter = Player.GetLocalCharacter()
  tP1Weapons = Human.Inventory.GetAllWeapons(uCharacter)
  for i, uWeapon in ipairs(tP1Weapons) do
    Human.Inventory.DropWeapon(uCharacter, uWeapon)
    x, y, z = Object.GetPosition(uCharacter)
    Object.DisablePhysics(uWeapon)
    Object.SetPosition(uWeapon, x, y - 5, z)
  end
  Human.Inventory.SetAllWeapons(uCharacter, Pg.GetGuidByName("Anti-Material Rifle"))
  Object.SetInfiniteAmmo(uCharacter, true)
  self:_CreateEvent(Event.TimerRelative, {10}, function()
    self:_CreateEvent(Event.Boundary, {
      Player.GetPrimaryCharacter(),
      Pg.GetGuidByName("PMCCon034OutOfBounds"),
      "exit"
    }, function()
      self:_SetCancelMessage("[PmcCon034.Terms.CancelOOB]")
      MrxVoSequence.Start({
        "Fiona-In-Mission-MinorContract-Pmc31-10",
        {
          self.Cancel,
          {self}
        }
      })
    end)
  end)
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
    x, y, z = Object.GetPosition(uCharacter)
    Object.DisablePhysics(uWeapon)
    Object.SetPosition(uWeapon, x, y - 5, z)
  end
  Human.Inventory.SetAllWeapons(uCharacter, Pg.GetGuidByName("Anti-Material Rifle"))
  Object.SetInfiniteAmmo(uCharacter, true)
end

function _AttachStatue(self)
  local HeloGuid = Pg.GetGuidByName("UH1 Transport (PMC) 0x00126e26")
  local x, y, z = Object.GetPosition(HeloGuid)
  uCargo = Pg.Spawn("_pmcoutpost_statueSolanobust_lowHP", x, y + 200, z)
  self:_CreateEvent(Event.ObjectHibernation, {HeloGuid, "awake"}, function()
    self:_CreateEvent(Event.ObjectHibernation, {uCargo, "awake"}, function()
      _DeployWinch(HeloGuid, uCargo, self)
      self:CreateChild({
        sName = "WinchStatue",
        sModuleName = "MrxTaskObjectiveDestroy",
        vTgtInclude = uCargo,
        sDspShortDesc = "[PmcCon034.Objectives.DestroyBonusStatue]",
        bOptional = true,
        fOnComplete = function()
          oStatueTimer = self:_CreateEvent(Event.TimerRelative, {10}, _AttachStatue, {self})
          _MinusTime(self)
        end
      })
    end)
  end)
end

function _DeployWinch(uGuid, uCargo, self)
  Object.SetWinchState(uGuid, "deployed")
  self:_CreateEvent(Event.TimerRelative, {0.1}, AttachCargo, {uGuid, uCargo})
end

function AttachCargo(uGuid, uCargo)
  Object.AttachCargoToWinch(uCargo, uGuid)
end

function _MinusTime(self)
  if PauseTimer then
    PauseTimer:Stop()
  end
  if MainTimerPause then
    Event.Delete(MainTimerPause)
  end
  self.CourseTimer:Pause()
  MainTimerPause = self:_CreateEvent(Event.TimerRelative, {5}, function()
    self.CourseTimer:Resume()
  end)
  Hud.ObjectiveTray:SetSlotToText({
    nSlot = 3,
    sText = "[green][PmcCon031.Terms.BonusTimePlus]"
  })
  self:_CreateEvent(Event.TimerRelative, {5}, function()
    Hud.ObjectiveTray:ClearSlot({vPlayer = nil, nSlot = 3})
  end)
  _FixTimers(self)
end

function _CallTruckTarget(self)
  _TruckTarget(self)
  self:_CreateEvent(Event.TimerRelative, {43}, _CallTruckTarget, {self})
end

function _TruckTarget(self)
  TruckSpawn = Pg.Spawn("El Grande (Driver)", 2663.6062, -12.248225, -938.5005, false, true)
  StatueSpawn = Pg.Spawn("_pmcoutpost_statueSolanobust_lowHP", 2665.4568, -13.856117, -940.5367, 0, false, true)
  Object.SetTransformToObject(TruckSpawn, Pg.GetGuidByName("TruckMoveLoc"))
  Object.SetTransformToObject(StatueSpawn, Pg.GetGuidByName("StatueMoveLoc"))
  sTruckObjName = "TruckStatue" .. iTruckTargetNum
  iTruckTargetNum = iTruckTargetNum + 1
  TruckObj = self:CreateChild({
    sName = sTruckObjName,
    sModuleName = "MrxTaskObjectiveDestroy",
    vTgtInclude = StatueSpawn,
    sDspShortDesc = "[PmcCon034.Objectives.DestroyBonusStatue]",
    bOptional = true,
    fOnComplete = function()
      _MinusTime(self)
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
  })
  self:_CreateEvent(Event.TimerRelative, {2}, function()
    Ai.Goal({
      AIGuid = Vehicle.GetDriver(TruckSpawn),
      Goal = "PathMove",
      Target = Pg.GetGuidByName("Path 0x00126fc1"),
      Priority = "HiPri",
      Haste = 0.15,
      Callback = _KillTruckStatue,
      CallbackData = {
        TruckSpawn,
        StatueSpawn,
        TruckObj
      }
    })
  end)
end

function _KillTruckStatue(Truck, Statue, TruckObj)
  Object.Remove(Truck)
  Object.Remove(Statue)
  TruckObj.Cancel(TruckObj)
end

function PlayMusic(self, nTimeLimit)
  MrxMusic.PlaySpecialMusic("mu_mission_pmccon034_01")
  SecondsTilSpeedUp = nTimeLimit - 25
  uMusicStartEvent = self:_CreateEvent(Event.TimerRelative, {SecondsTilSpeedUp}, function()
    MrxMusic.PlaySpecialMusic("mu_mission_pmccon034_02")
  end)
  uMusicEndEvent = self:_CreateEvent(Event.TimerRelative, {nTimeLimit}, function()
    MrxMusic.PlaySpecialMusic("mu_mission_pmccon034_01")
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

function _FixTimers(self)
  TimeLeft = self.CourseTimer:GetTime()
  Debug.Printf("Time Left on timer - " .. TimeLeft)
  TimeLeft = nTimeLimit - TimeLeft
  Debug.Printf("Time Left to complete mission successfully - " .. TimeLeft)
  TimeLeft = TimeLeft + 5
  if uCountdownFail then
    Event.Delete(uCountdownFail)
    uCountdownFail = self:_CreateEvent(Event.TimerRelative, {TimeLeft}, function()
      FailureVO(self)
      uCountdownFail = nil
    end)
  end
  if uCountdown5 then
    Event.Delete(uCountdown5)
    uCountdown5 = self:_CreateEvent(Event.TimerRelative, {
      TimeLeft - 6.5
    }, function()
      MrxVoSequence.Start("Fiona-In-Mission-MinorContract-Pmc31-13")
      uCountdown5 = nil
    end)
  end
  if uCountdown15 then
    Event.Delete(uCountdown15)
    uCountdown15 = self:_CreateEvent(Event.TimerRelative, {
      TimeLeft - 16.5
    }, function()
      MrxVoSequence.Start("Fiona-In-Mission-MinorContract-Pmc31-12")
      uCountdown15 = nil
    end)
  end
  if uCountdown30 then
    Event.Delete(uCountdown30)
    uCountdown30 = self:_CreateEvent(Event.TimerRelative, {
      TimeLeft - 31.5
    }, function()
      MrxVoSequence.Start("Fiona-In-Mission-MinorContract-Pmc11-01")
      uCountdown30 = nil
    end)
  end
  if uMusicStartEvent then
    Event.Delete(uMusicStartEvent)
    uMusicStartEvent = self:_CreateEvent(Event.TimerRelative, {
      TimeLeft - 25
    }, function()
      MrxMusic.PlaySpecialMusic("mu_mission_pmccon034_02")
      uMusicStartEvent = nil
    end)
  end
  if uMusicEndEvent then
    Event.Delete(uMusicEndEvent)
    uMusicEndEvent = self:_CreateEvent(Event.TimerRelative, {
      TimeLeft - 25
    }, function()
      MrxMusic.PlaySpecialMusic("mu_mission_pmccon034_01")
      uMusicEndEvent = nil
    end)
  end
  if uCountdownHero then
    Event.Delete(uCountdownHero)
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
      TimeLeft - 3
    }, function()
      uCountdownHero = nil
      MrxVoSequence.Start({
        {
          mattias = MrxUtil.GetRandomTableElement(tMattiasNegativeVO),
          jennifer = MrxUtil.GetRandomTableElement(tJenNegativeVO),
          chris = MrxUtil.GetRandomTableElement(tChrisNegativeVO)
        }
      })
    end)
  end
end

function CompleteVO(self)
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
  self.CourseTimer:Pause()
  tPossibleVO = {
    "Fiona-In-Mission-MinorContract-Pmc31-02",
    "Fiona-In-Mission-MinorContract-Pmc31-20",
    "Fiona-In-Mission-MinorContract-Pmc31-21",
    "Fiona-In-Mission-MinorContract-Pmc32-01",
    "Fiona-In-Mission-MinorContract-Pmc32-02",
    "Fiona-In-Mission-MinorContract-Pmc34-01",
    "Fiona-In-Mission-MinorContract-Pmc31-33",
    "Fiona-In-Mission-MinorContract-Pmc31-35",
    "Fiona-In-Mission-MinorContract-Pmc33-01",
    "Fiona-In-Mission-MinorContract-Pmc33-01"
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
    "Fiona-In-Mission-MinorContract-Pmc32-03"
  }
  if WifMissionFlow.HasKey("JetCon001") then
    table.insert(tPossibleVO, "Fiona-In-Mission-MinorContract-Pmc31-17")
  end
  sVOLine = MrxUtil.GetRandomTableElement(tPossibleVO)
  MrxVoSequence.Start({sVOLine})
end

function OnPlayerJoined(self, iPlayerId, uPlayerGuid, uCharGuid)
  self:_CreateEvent(Event.TimerRelative, {10}, function()
    if Player.GetSecondaryCharacter() then
      tLocalP2Weapons = Human.Inventory.GetAllWeapons(Player.GetSecondaryCharacter())
      self:_CreateEvent(Event.Boundary, {
        Player.GetSecondaryCharacter(),
        Pg.GetGuidByName("PMCCon034OutOfBounds"),
        "exit"
      }, function()
        self:_SetCancelMessage("[PmcCon034.Terms.CancelOOB]")
        MrxVoSequence.Start({
          "Fiona-In-Mission-MinorContract-Pmc31-10",
          {
            self.Cancel,
            {self}
          }
        })
      end)
    end
  end)
end

function Cleanup(self)
  Event.Delete(_evClientJoinedPMC034)
  uCharacter = Player.GetSecondaryCharacter()
  if uCharacter then
    for i, uWeapon in ipairs(tLocalP2Weapons) do
      Debug.Printf(tostring(uWeapon))
    end
  end
  Hud.SupportMenu:SetShootingGalleryMode({bEnable = false})
  if TruckSpawn then
    Object.Remove(TruckSpawn)
  end
  if StatueSpawn then
    Object.Remove(StatueSpawn)
  end
  if uCargo then
    Object.Remove(uCargo)
  end
  uHelo = Pg.GetGuidByName("UH1 Transport (PMC) 0x00126e26")
  if uHelo then
    Object.SetPosition(uHelo, 2632, 155, -1000, false)
  end
  Hud.ObjectiveTray:ClearSlot({nSlot = 1})
  Hud.ObjectiveTray:ClearSlot({nSlot = 2})
  Hud.ObjectiveTray:ClearSlot({nSlot = 3})
  if self.CourseTimer then
    self.CourseTimer:Stop()
  end
  if oStatueTimer then
    Event.Delete(oStatueTimer)
  end
  MrxTaskContract.Cleanup(self)
  for i, sLayerName in ipairs(tLayersToAdd) do
    MrxLayerManager.MarkForRemoval(sLayerName)
  end
end

function Complete(self)
  uCharacter = Player.GetLocalCharacter()
  Object.SetInfiniteAmmo(uCharacter, false)
  if uMusicStartEvent then
    Event.Delete(uMusicStartEvent)
  end
  Net.SendCustomEvent("PmcCon034", NETEVENT_RETURNWEAPONS, {}, true)
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
  Net.SendCustomEvent("PmcCon034", NETEVENT_RETURNWEAPONS, {}, true)
  Human.Inventory.SetAllWeapons(uCharacter, tP1Weapons)
  Human.DisableWeapons(uCharacter)
  if Player.GetSecondaryCharacter() then
    Human.DisableWeapons(Player.GetSecondaryCharacter())
  end
  MrxTaskContract.Cancel(self)
end
