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
tCarsToDelete = {}
bDamageWarning = false
CoolDown = 0
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
      Player.SetAimMode(Player.GetPrimaryPlayer(), true)
    end
  end
end

function LoadAssets(self, tSaveData)
  tLayersToAdd = {
    "Vz_State_PmcCon031",
    "Vz_state_PMC"
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
      Net.SendCustomEvent("PmcCon031", NETEVENT_SETSTARTUPWEAPONS, {}, true)
    end)
  end
end

function Activated(self)
  MrxTaskContract.Activated(self)
  _evClientJoinedPMC031 = Event.CreatePersistent(Event.ScriptEvent, {
    "mpPlayerJoin",
    function(tData)
      return Net.IsServer() and not Player.IsLocal(tData[1])
    end
  }, Net.SendCustomEvent, {
    "PmcCon031",
    NETEVENT_SETSTARTUPWEAPONS,
    {},
    true
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
  nCompletions = self:GetNumCompletions()
  PointDist = 2.5
  NumCars = 10
  if nCompletions == 0 then
    nTimeLimit = 240
    sTimeToBeat = "4:00"
  elseif nCompletions == 1 then
    nTimeLimit = 150
    sTimeToBeat = "2:30"
  elseif nCompletions >= 2 then
    nTimeLimit = 90
    sTimeToBeat = "1:30"
  end
  self:_SetupObjective(nTimeLimit, nQuota)
end

function _SetupObjective(self, nTimeLimit, nQuota)
  if Player.GetSecondaryCharacter() then
    tLocalP2Weapons = Human.Inventory.GetAllWeapons(Player.GetSecondaryCharacter())
  end
  Player.SetAimMode(Player.GetPrimaryPlayer(), false)
  if Player.GetSecondaryPlayer() then
    Player.SetAimMode(Player.GetSecondaryPlayer(), false)
  end
  self.CourseTimer = MrxTimer:Create({
    nStartTime = nTimeLimit,
    nStopTime = 0,
    nStep = 0.1,
    nWarning = 10,
    bUseTenths = true,
    iTray = 1,
    tDoneCallbacks = {
      {
        TimeUp,
        {self}
      }
    }
  })
  _CountDownVOSetup(self, nTimeLimit)
  Debug.Printf("About to start timer!")
  self.CourseTimer:Start()
  PlayMusic(self, nTimeLimit)
  nHealth = Object.GetHealth(Player.GetLocalCharacter())
  self:_CreateEvent(Event.ObjectHealth, {
    Player.GetLocalCharacter(),
    "<",
    nHealth
  }, DamageWarningVO)
  iGlsDead = 0
  uGLDeath1 = self:_CreateEvent(Event.ObjectDeath, {
    Pg.GetGuidByName("Emplaced GL 0x00126f78")
  }, function()
    DamageWarningVO()
    iGlsDead = iGlsDead + 1
    if iGlsDead == 2 then
      self:_SetCancelMessage("[PmcCon031.Terms.CancelTurrets]")
      Cancel(self)
    end
  end)
  uGLDeath2 = self:_CreateEvent(Event.ObjectDeath, {
    Pg.GetGuidByName("Emplaced GL 0x0012d383")
  }, function()
    DamageWarningVO()
    iGlsDead = iGlsDead + 1
    if iGlsDead == 2 then
      self:_SetCancelMessage("[PmcCon031.Terms.CancelTurrets]")
      Cancel(self)
    end
  end)
  iMGsDead = 0
  uMGDeath1 = self:_CreateEvent(Event.ObjectDeath, {
    Pg.GetGuidByName("Emplaced MG 0x00126f7a")
  }, function()
    iMGsDead = iMGsDead + 1
    if iMGsDead == 2 then
      self:_SetCancelMessage("[PmcCon031.Terms.CancelTurrets]")
      Cancel(self)
    end
  end)
  uMGDeath2 = self:_CreateEvent(Event.ObjectDeath, {
    Pg.GetGuidByName("Emplaced MG 0x0012d380")
  }, function()
    iMGsDead = iMGsDead + 1
    if iMGsDead == 2 then
      self:_SetCancelMessage("[PmcCon031.Terms.CancelTurrets]")
      Cancel(self)
    end
  end)
  uRRDeath2 = self:_CreateEvent(Event.ObjectDeath, {
    Pg.GetGuidByName("Emplaced Recoiless Rifle 0x0012d382")
  }, function()
    DamageWarningVO()
  end)
  uRRDeath2 = self:_CreateEvent(Event.ObjectDeath, {
    Pg.GetGuidByName("Emplaced Recoiless Rifle 0x00126f75")
  }, function()
    DamageWarningVO()
  end)
  _MoveToMG(self)
end

function _MoveToMG(self)
  if nCompletions < 2 then
    tMGVO = {
      "Fiona-In-Mission-MinorContract-Pmc31-32",
      1,
      "Fiona-In-Mission-MinorContract-Pmc31-29"
    }
  else
    tMGVO = {
      "Fiona-In-Mission-MinorContract-Pmc31-29"
    }
  end
  self:CreateChild({
    sName = "MoveToMG",
    sDspShortDesc = "[PmcCon031.Objectives.MoveMG]",
    sModuleName = "MrxTaskObjectiveDeliver",
    vTgtInclude = Player.GetAnyCharacter(),
    bDspBlp = true,
    vDestLoc = "PmcCon031_MGPoint",
    fDist = 3,
    bStop = true,
    fOnComplete = function()
      _DestroyStatues(self)
    end,
    vVoSeqOnAdd = tMGVO
  })
end

function _DestroyStatues(self)
  local nQuota = 0
  tStatueTargets = {
    "_pmcoutpost_column_noreflection 0x00127085",
    "_pmcoutpost_column_noreflection 0x00127086",
    "_pmcoutpost_column_noreflection 0x00127087",
    "_pmcoutpost_column_noreflection 0x00127088",
    "_pmcoutpost_column_noreflection 0x00127089",
    "_pmcoutpost_column_noreflection 0x0012708a",
    "_pmcoutpost_column_noreflection 0x0012708b",
    "_pmcoutpost_column_noreflection 0x0012708c",
    "_pmcoutpost_column_noreflection 0x0012708d",
    "_pmcoutpost_column_noreflection 0x0012d09f",
    "_pmcoutpost_column_noreflection 0x0012d0a1",
    "_pmcoutpost_column_noreflection 0x0012d0a3",
    "_pmcoutpost_column_noreflection 0x0012d0a4",
    "_pmcoutpost_column_noreflection 0x0012d0a5",
    "_pmcoutpost_column_noreflection 0x0012d0a6",
    "_pmcoutpost_column_noreflection 0x0012d0a7",
    "_pmcoutpost_column_noreflection 0x0012d0a8",
    "_pmcoutpost_column_noreflection 0x0012d0a9",
    "_pmcoutpost_column_noreflection 0x0012d0aa",
    "_pmcoutpost_column_noreflection 0x0012d0ab",
    "_pmcoutpost_column_noreflection 0x0012d0ac",
    "_pmcoutpost_column_noreflection 0x0012d0ad",
    "_pmcoutpost_column_noreflection 0x0012d0ae",
    "_pmcoutpost_column_noreflection 0x0012d0af",
    "_pmcoutpost_column_noreflection 0x0012d0b0",
    "_pmcoutpost_column_noreflection 0x0012d0b1",
    "_pmcoutpost_column_noreflection 0x0012d0b2",
    "_pmcoutpost_column_noreflection 0x0012d0b3",
    "_pmcoutpost_column_noreflection 0x0012d0b4",
    "_pmcoutpost_column_noreflection 0x0012d0b5",
    "_pmcoutpost_column_noreflection 0x0012d0b6",
    "_pmcoutpost_column_noreflection 0x0012d0b7",
    "_pmcoutpost_column_noreflection 0x0012d0b8",
    "_pmcoutpost_column_noreflection 0x0012d0b9",
    "_pmcoutpost_column_noreflection 0x0012d0ba",
    "_pmcoutpost_column_noreflection 0x0012d0bb",
    "_pmcoutpost_column_noreflection 0x0012d0bc",
    "_pmcoutpost_column_noreflection 0x0012d0bd",
    "_pmcoutpost_column_noreflection 0x0012d0bf",
    "_pmcoutpost_column_noreflection 0x0012d0c0",
    "_pmcoutpost_column_noreflection 0x0012d0c1",
    "_pmcoutpost_column_noreflection 0x0012d0c2",
    "_pmcoutpost_column_noreflection 0x0012d0c3",
    "_pmcoutpost_column_noreflection 0x0012d0c4",
    "_pmcoutpost_column_noreflection 0x0012d0c5",
    "_pmcoutpost_column_noreflection 0x0012d0c6",
    "_pmcoutpost_column_noreflection 0x0012d0c7",
    "_pmcoutpost_column_noreflection 0x0012d0c8",
    "_pmcoutpost_column_noreflection 0x0012d0c9",
    "_pmcoutpost_column_noreflection 0x0012d0cb"
  }
  for i, sColumn in ipairs(tStatueTargets) do
    if Object.IsAlive(Pg.GetGuidByName(sColumn)) then
      nQuota = nQuota + 1
      Debug.Printf("nQuota = " .. nQuota)
    end
  end
  if Math.randi(1, 15) == 1 then
    self:_CreateEvent(Event.TimerRelative, {10}, MrxVoSequence.Start, {
      {
        "Fiona-In-Mission-MinorContract-Pmc33-02"
      }
    })
  end
  oFakeMachineGunObj = self:CreateChild({
    sName = "Fake Destroy Statues",
    sModuleName = "MrxTaskObjectiveDestroy",
    vTgtInclude = "MGStatueLoc",
    bDspMsg = false,
    bDspDescPda = false,
    bDspBlpWld = false,
    bDspBlpRdr = true,
    bDspBlpPda = true,
    sDspShortDesc = "[PmcCon031.Objectives.TakeOut]"
  })
  oMachineGunObj = self:CreateChild({
    sName = "Destroy Statues",
    sModuleName = "MrxTaskObjectiveDestroy",
    sTgtLabelFilter = "PMCCon031Statue",
    vTgtInclude = tStatueTargets,
    nQuota = nQuota,
    bDspBlp = false,
    sDspShortDesc = "[PmcCon031.Objectives.TakeOut]",
    tOnPartComplete = {
      {
        StatueKilled,
        {self}
      }
    },
    fOnComplete = function()
      _MoveToRR(self)
      oFakeMachineGunObj.Complete(oFakeMachineGunObj)
      if oFakeMachineGunObj2 then
        oFakeMachineGunObj2.Complete(oFakeMachineGunObj2)
      end
    end
  })
  if nQuota <= 0 then
    oMachineGunObj.Complete(oMachineGunObj)
  end
end

function StatueKilled(self, iGuid)
  if oMachineGunObj._nCompleted == oMachineGunObj._nQuota - 5 then
    oFakeMachineGunObj:Configure({bDspBlpRdr = false})
    oFakeMachineGunObj2 = self:CreateChild({
      sName = "Destroy Statues Blip",
      sModuleName = "MrxTaskObjectiveDestroy",
      vTgtInclude = tStatueTargets,
      bDspMsg = false,
      bDspDescPda = false,
      bDspBlpWld = true,
      bDspBlpRdr = false,
      bDspBlpPda = false,
      sDspShortDesc = "[PmcCon031.Objectives.TakeOut]"
    })
    Debug.Printf("^^^^^^^^^^^^^^^^^^^^5 statues left")
  end
end

function Obj_MGStatues_StatueKilled(self)
  Debug.Printf("$$$$$$$$$$INside Obj_MGStatues_StatueKilled, self.curObj._nCompleted = " .. tostring(self.curObj._nCompleted))
  if self.curObj._nCompleted == self.curObj._nQuota - 4 then
    Debug.Printf("$$$$$$$$$$INside Obj_MGStatues_StatueKilled's if statement, self.curObj._nCompleted = " .. tostring(self.curObj._nCompleted))
    self.curObj:Configure({bDspBlp = true})
  end
end

function _MoveToRR(self)
  Event.Delete(uMGDeath1)
  Event.Delete(uMGDeath2)
  CarsMissed = 0
  self:CreateChild({
    sName = "MoveToRR",
    sDspShortDesc = "[PmcCon031.Objectives.MoveRR]",
    sModuleName = "MrxTaskObjectiveDeliver",
    vTgtInclude = Player.GetAnyCharacter(),
    bDspBlp = true,
    vDestLoc = "PmcCon031_RRPoint",
    fDist = 6,
    bStop = true,
    fOnComplete = function()
      _SpawnCar(self)
    end,
    vVoSeqOnAdd = {
      "Fiona-In-Mission-MinorContract-Pmc31-36"
    }
  })
end

function _MovetoGL(self)
  Hud.ObjectiveTray:ClearSlot({nSlot = 3})
  self:CreateChild({
    sName = "MoveToGL",
    sDspShortDesc = "[PmcCon031.Objectives.MoveGL]",
    sModuleName = "MrxTaskObjectiveDeliver",
    vTgtInclude = Player.GetAnyCharacter(),
    bDspBlp = true,
    vDestLoc = "PmcCon031_GLPoint",
    fDist = 2,
    bStop = true,
    fOnComplete = function()
      _DestroyStatuesGL(self)
    end,
    vVoSeqOnAdd = {
      "Fiona-In-Mission-MinorContract-Pmc31-31"
    }
  })
end

function _DestroyStatuesGL(self)
  local nQuota = 0
  tGrenadeStatueTargets = {
    "_pmcoutpost_column_noreflection_large 0x0012d0cc",
    "_pmcoutpost_column_noreflection_large 0x0012d0cd",
    "_pmcoutpost_column_noreflection_large 0x0012d0ce",
    "_pmcoutpost_column_noreflection_large 0x0012d0cf",
    "_pmcoutpost_column_noreflection_large 0x0012d0d1",
    "_pmcoutpost_column_noreflection_large 0x0012d0d2",
    "_pmcoutpost_column_noreflection_large 0x0012d0d3",
    "_pmcoutpost_column_noreflection_large 0x0012d0d7",
    "_pmcoutpost_column_noreflection_large 0x0012d0d9",
    "_pmcoutpost_column_noreflection_large 0x0012d0da",
    "_pmcoutpost_column_noreflection_large 0x0012d0db",
    "_pmcoutpost_column_noreflection_large 0x0012d0e0",
    "_pmcoutpost_column_noreflection_large 0x0012d0e3",
    "_pmcoutpost_column_noreflection_large 0x0012d0e4",
    "_pmcoutpost_column_noreflection_large 0x0012d0e6",
    "_pmcoutpost_column_noreflection_large 0x0012d0e7",
    "_pmcoutpost_column_noreflection_large 0x0012d0e8",
    "_pmcoutpost_column_noreflection_large 0x0012d0e9",
    "_pmcoutpost_column_noreflection_large 0x0012d0ea",
    "_pmcoutpost_column_noreflection_large 0x0012d0eb",
    "_pmcoutpost_column_noreflection_large 0x0012d0ec",
    "_pmcoutpost_column_noreflection_large 0x0012d0ee",
    "_pmcoutpost_column_noreflection_large 0x0012d0ef",
    "_pmcoutpost_column_noreflection_large 0x0012d0f0",
    "_pmcoutpost_column_noreflection_large 0x0012d0f1",
    "_pmcoutpost_column_noreflection_large 0x0012d0f3",
    "_pmcoutpost_column_noreflection_large 0x0012d0f4",
    "_pmcoutpost_column_noreflection_large 0x0012d0f5",
    "_pmcoutpost_column_noreflection_large 0x0012d0f6",
    "_pmcoutpost_column_noreflection_large 0x0012d0f7",
    "_pmcoutpost_column_noreflection_large 0x0012d0f8",
    "_pmcoutpost_column_noreflection_large 0x0012d0fa",
    "_pmcoutpost_column_noreflection_large 0x0012d0fb",
    "_pmcoutpost_column_noreflection_large 0x0012d0fc",
    "_pmcoutpost_column_noreflection_large 0x0012d0fd",
    "_pmcoutpost_column_noreflection_large 0x0012d1d0",
    "_pmcoutpost_column_noreflection_large 0x0012d1d2",
    "_pmcoutpost_column_noreflection_large 0x0012d1d3",
    "_pmcoutpost_column_noreflection_large 0x0012d1d5",
    "_pmcoutpost_column_noreflection_large 0x0012d1d6",
    "_pmcoutpost_column_noreflection_large 0x0012d1d7",
    "_pmcoutpost_column_noreflection_large 0x0012d1d8",
    "_pmcoutpost_column_noreflection_large 0x0012d1d9",
    "_pmcoutpost_column_noreflection_large 0x0012d1da",
    "_pmcoutpost_column_noreflection_large 0x0012d1db",
    "_pmcoutpost_column_noreflection_large 0x0012d1dc",
    "_pmcoutpost_column_noreflection_large 0x0012d1dd",
    "_pmcoutpost_column_noreflection_large 0x0012d1de",
    "_pmcoutpost_column_noreflection_large 0x0012d1df",
    "_pmcoutpost_column_noreflection_large 0x0012d1e0",
    "_pmcoutpost_column_noreflection_large 0x0012d1e1",
    "_pmcoutpost_column_noreflection_large 0x0012d1e2",
    "_pmcoutpost_column_noreflection_large 0x0012d1e3",
    "_pmcoutpost_column_noreflection_large 0x0012d1e4",
    "_pmcoutpost_column_noreflection_large 0x0012d1e5"
  }
  for i, sColumn in ipairs(tGrenadeStatueTargets) do
    if Object.IsAlive(Pg.GetGuidByName(sColumn)) then
      nQuota = nQuota + 1
      Debug.Printf("nQuota = " .. nQuota)
    end
  end
  nQuota = nQuota - 5
  oFakeGrenadeObj = self:CreateChild({
    sName = "Fake Destroy Statues",
    sModuleName = "MrxTaskObjectiveDestroy",
    vTgtInclude = {
      "RRStatueLoc2"
    },
    bDspMsg = false,
    bDspBlp = true,
    bDspDescPda = false,
    bDspBlpWld = false,
    bDspBlpRdr = true,
    bDspBlpPda = true,
    sDspShortDesc = "[PmcCon031.Objectives.TakeOut]"
  })
  oFakeGrenadeObj1 = self:CreateChild({
    sName = "Fake Destroy Statues2",
    sModuleName = "MrxTaskObjectiveDestroy",
    vTgtInclude = {
      "RRStatueLoc1"
    },
    bDspMsg = false,
    bDspBlp = true,
    bDspDescPda = false,
    bDspBlpWld = false,
    bDspBlpRdr = true,
    bDspBlpPda = true,
    sDspShortDesc = "[PmcCon031.Objectives.TakeOut]"
  })
  oGrenadeObj = self:CreateChild({
    sName = "Emplaced Grenade Destroy",
    sModuleName = "MrxTaskObjectiveDestroy",
    sTgtLabelFilter = "PMCCon031StatueGL",
    nQuota = nQuota,
    bDspBlp = true,
    sDspShortDesc = "[PmcCon031.Objectives.TakeOut]",
    tOnPartComplete = {
      {
        StatueKilledGrenade,
        {self}
      }
    },
    fOnComplete = function()
      if uCountdown5 then
        Event.Delete(uCountdown5)
      end
      if uCountdown15 then
        Event.Delete(uCountdown15)
      end
      if uCountdown30 then
        Event.Delete(uCountdown30)
      end
      oFakeGrenadeObj.Complete(oFakeGrenadeObj)
      oFakeGrenadeObj1.Complete(oFakeGrenadeObj1)
      if oFakeGrenadeObj2 then
        oFakeGrenadeObj2.Complete(oFakeGrenadeObj2)
      end
      CompleteVO(self)
    end
  })
  if nQuota <= 0 then
    oGrenadeObj.Complete(oGrenadeObj)
  end
end

function StatueKilledGrenade(self, iGuid)
  if oGrenadeObj._nCompleted == oGrenadeObj._nQuota - 10 then
    oFakeGrenadeObj2 = self:CreateChild({
      sName = "Destroy Grenade Statues Blip",
      sModuleName = "MrxTaskObjectiveDestroy",
      vTgtInclude = tGrenadeStatueTargets,
      bDspMsg = false,
      bDspDescPda = false,
      bDspBlpWld = true,
      bDspBlpRdr = false,
      bDspBlpPda = false,
      sDspShortDesc = "[PmcCon031.Objectives.TakeOut]"
    })
    Debug.Printf("^^^^^^^^^^^^^^^^^^^^5 statues left")
  end
end

function _SpawnCar(self)
  NumCars = NumCars - 1
  Hud.ObjectiveTray:SetSlotToText({
    nSlot = 3,
    sText = "[PmcCon031.Terms.CarCount] " .. NumCars
  })
  if uEndCarObjTimer then
    Event.Delete(uEndCarObjTimer)
  end
  local i = Math.randi(1, 4)
  local x, y, z
  if i == 1 then
    sCar = "L300 (Fling Forward)"
    x, y, z = 2759.982, -14.324945, -867.9881
  elseif i == 2 then
    sCar = "R90 (Fling Right)"
    x, y, z = 2821.7236, -14.031256, -838.6553
  elseif i == 3 then
    sCar = "R90 (Fling Left)"
    x, y, z = 2688.671, -14.32493, -809.67426
  elseif i == 4 then
    sCar = "L300 (Fling Backward)"
    x, y, z = 2773.6199, -21.570244, -710.3076
  end
  uTempCar = Pg.Spawn(sCar, x, y, z)
  table.insert(tCarsToDelete, uTempCar)
  if NumCars == 0 then
    uRecurse = self:_CreateEvent(Event.TimerRelative, {10}, _MovetoGL, {self})
  else
    uRecurse = self:_CreateEvent(Event.TimerRelative, {10}, _SpawnCar, {self})
  end
  sObjName = "TempStatue" .. NumCars
  uTempObj = self:CreateChild({
    sName = sObjName,
    sModuleName = "MrxTaskObjectiveDestroy",
    vTgtInclude = uTempCar,
    sDspShortDesc = "[PmcCon031.Objectives.DestroyCar]",
    fOnComplete = function()
      _CarHit(self)
    end
  })
  uEndCarObjTimer = self:_CreateEvent(Event.TimerRelative, {9}, function()
    uTempObj.Cancel(uTempObj)
    CarsMissed = CarsMissed + 1
    if CarsMissed == 4 then
      tChrisNegativeVO = {
        "Chris.BadNews03",
        "Chris.Misc.Negative01",
        "Chris.Misc.Negative02",
        "Chris.Misc.Negative03",
        "Chris.Misc.Negative04",
        "Chris.Misc.Negative05"
      }
      tMattiasNegativeVO = {
        "Mattias.BadNews01",
        "Mattias.Misc.Negative05",
        "Mattias.Misc.Negative01",
        "Mattias.Misc.Negative04",
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
        "Jen.Negative04",
        "Jen.Negative05",
        "Jen.BadNews01"
      }
      MrxVoSequence.Start({
        {
          mattias = MrxUtil.GetRandomTableElement(tMattiasNegativeVO),
          jennifer = MrxUtil.GetRandomTableElement(tJenNegativeVO),
          chris = MrxUtil.GetRandomTableElement(tChrisNegativeVO)
        }
      })
    end
  end)
  uCongratVO = self:_CreateEvent(Event.ObjectDeath, {uTempCar}, function()
    if CoolDown == 0 then
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
      CoolDown = 1
      self:_CreateEvent(Event.TimerRelative, {25}, function()
        CoolDown = 0
      end)
    end
  end)
  self:_CreateEvent(Event.TimerRelative, {5}, Event.Delete, {uCongratVO})
end

function _CarHit(self)
  CarsMissed = 0
  if MainTimerPause then
    Event.Delete(MainTimerPause)
  end
  if MainTimer then
    Event.Delete(MainTimer)
  end
  _FixTimers(self)
  self.CourseTimer:Pause()
  MainTimerPause = self:_CreateEvent(Event.TimerRelative, {5}, function()
  end)
  Hud.ObjectiveTray:SetSlotToText({
    nSlot = 2,
    sText = "[green][PmcCon031.Terms.BonusTimePlus]"
  })
  MainTimer = self:_CreateEvent(Event.TimerRelative, {5}, function()
    self.CourseTimer:Resume()
    Hud.ObjectiveTray:ClearSlot({vPlayer = nil, nSlot = 2})
  end)
  Event.Delete(uRecurse)
  if NumCars == 0 then
    _MovetoGL(self)
  else
    _SpawnCar(self)
  end
end

function PlayMusic(self, nTimeLimit)
  MrxMusic.PlaySpecialMusic("mu_mission_pmccon031_01")
  SecondsTilSpeedUp = nTimeLimit - 25
  uMusicStartEvent = self:_CreateEvent(Event.TimerRelative, {SecondsTilSpeedUp}, function()
    MrxMusic.PlaySpecialMusic("mu_mission_pmccon031_02")
  end)
  uMusicEndEvent = self:_CreateEvent(Event.TimerRelative, {nTimeLimit}, function()
    MrxMusic.PlaySpecialMusic("mu_mission_pmccon031_01")
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
  Object.SetInfiniteAmmo(uCharacter, true)
  Player.SetAimMode(Player.GetPrimaryPlayer(), false)
  Player.SetAimMode(Player.GetSecondaryPlayer(), false)
end

function _MoveWeapons(uCharacter, tWeapons)
  for i, uWeapon in ipairs(tWeapons) do
    x, y, z = Object.GetPosition(uCharacter)
    Object.DisablePhysics(uWeapon)
    Object.SetPosition(uWeapon, x, y - 5, z)
  end
end

function _SetupBorderWeapons(uBorderName, uWeaponName, self)
  tWeapons = MrxShootingGallery.RemoveWeapons(Player.GetLocalCharacter())
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

function DamageWarningVO()
  if bDamageWarning == false then
    bDamageWarning = true
    MrxVoSequence.Start({
      "Fiona-In-Mission-MinorContract-Pmc31-19"
    })
  end
end

function _CountDownVOSetup(self, nTimeLimit)
  uCountdown5 = self:_CreateEvent(Event.TimerRelative, {
    nTimeLimit - 5
  }, MrxVoSequence.Start, {
    "Fiona-In-Mission-MinorContract-Pmc31-13"
  })
  uCountdown15 = self:_CreateEvent(Event.TimerRelative, {
    nTimeLimit - 15
  }, MrxVoSequence.Start, {
    "Fiona-In-Mission-MinorContract-Pmc31-12"
  })
  uCountdown30 = self:_CreateEvent(Event.TimerRelative, {
    nTimeLimit - 30
  }, MrxVoSequence.Start, {
    "Fiona-In-Mission-MinorContract-Pmc11-01"
  })
end

function _FixTimers(self)
  TimeLeft = self.CourseTimer:GetTime()
  Debug.Printf("Timeleft = " .. TimeLeft)
  TimeLeft = TimeLeft + 5
  if uCountdown5 then
    Event.Delete(uCountdown5)
    uCountdown5 = self:_CreateEvent(Event.TimerRelative, {
      TimeLeft - 5
    }, function()
      MrxVoSequence.Start("Fiona-In-Mission-MinorContract-Pmc31-13")
      uCountdown5 = nil
    end)
  end
  if uCountdown15 then
    Event.Delete(uCountdown15)
    uCountdown15 = self:_CreateEvent(Event.TimerRelative, {
      TimeLeft - 15
    }, function()
      MrxVoSequence.Start("Fiona-In-Mission-MinorContract-Pmc31-12")
      uCountdown15 = nil
    end)
  end
  if uCountdown30 then
    Event.Delete(uCountdown30)
    uCountdown30 = self:_CreateEvent(Event.TimerRelative, {
      TimeLeft - 30
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
end

function CompleteVO(self)
  self.CourseTimer:Pause()
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

function TimeUp(self)
  if oGrenadeObj then
    oGrenadeObj.Cancel(oGrenadeObj)
  end
  self:_SetCancelMessage("[PmcCon031.Terms.CancelTime]")
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
  MrxVoSequence.Start({
    sVOLine,
    {
      Cancel,
      {self}
    }
  })
end

function Complete(self)
  uCharacter = Player.GetLocalCharacter()
  Object.SetInfiniteAmmo(uCharacter, false)
  if uMusicStartEvent then
    Event.Delete(uMusicStartEvent)
  end
  Net.SendCustomEvent("PmcCon031", NETEVENT_RETURNWEAPONS, {}, true)
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
  Net.SendCustomEvent("PmcCon031", NETEVENT_RETURNWEAPONS, {}, true)
  Human.Inventory.SetAllWeapons(uCharacter, tP1Weapons)
  Human.DisableWeapons(uCharacter)
  if Player.GetSecondaryCharacter() then
    Human.DisableWeapons(Player.GetSecondaryCharacter())
  end
  MrxTaskContract.Cancel(self)
end

function Cleanup(self)
  Event.Delete(_evClientJoinedPMC031)
  Event.Delete(_evMoveWeapons)
  if tCarsToDelete then
    for i, uCarToDelete in ipairs(tCarsToDelete) do
      if uCarToDelete then
        Object.Remove(uCarToDelete)
      end
    end
  end
  uCharacter = Player.GetSecondaryCharacter()
  if uCharacter then
    for i, uWeapon in ipairs(tLocalP2Weapons) do
      Debug.Printf(tostring(uWeapon))
    end
  end
  MrxLayerManager.MarkForRemoval("Vz_State_PmcCon031")
  Hud.SupportMenu:SetShootingGalleryMode({bEnable = false})
  if uTempObj then
    uTempObj.Cancel(uTempObj)
  end
  Hud.ObjectiveTray:ClearSlot({vPlayer = nil, nSlot = 1})
  Hud.ObjectiveTray:ClearSlot({vPlayer = nil, nSlot = 2})
  Hud.ObjectiveTray:ClearSlot({vPlayer = nil, nSlot = 3})
  Player.SetAimMode(Player.GetPrimaryPlayer(), true)
  if Player.GetSecondaryPlayer() then
    Player.SetAimMode(Player.GetSecondaryPlayer(), true)
  end
  MrxLayerManager.MarkForAddition("vz_state_pmc")
  self.CourseTimer:Stop()
  MrxTaskContract.Cleanup(self)
end
