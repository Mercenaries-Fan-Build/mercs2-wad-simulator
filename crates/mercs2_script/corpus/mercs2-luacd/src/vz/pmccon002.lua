inherit("MrxTaskContract")
import("MrxState")
import("MrxSubtitle")
import("MrxCinematic")
import("MrxGui")
import("DangerousBuilding")
import("MrxLayerManager")
import("MrxVoSequence")
import("MrxUtil")

function Activated(self)
  MrxTaskContract.Activated(self)
  self:_CreateEvent(Event.ObjectHibernation, {
    Player.GetLocalCharacter(),
    "awake"
  }, Start, {self})
end

function Start(self)
  oOfficeObjective = self:CreateChild({
    sName = "Find Blanco",
    sModuleName = "MrxTaskObjectiveAction",
    sActionLabel = "[ContextAction.EnterOffice]",
    vTgtInclude = "PMC002_Office",
    sDspShortDesc = "[PmcCon002.objective.office]",
    tOnPartComplete = {
      {
        PlayOfficeCinematic,
        {self}
      }
    }
  })
  uOilRig = Pg.GetGuidByName("PMC002 Oilrig")
  uBuildingA = Pg.GetGuidByName("PmcCon002_OilrigA")
  self:_CreateEvent(Event.ObjectHibernation, {uBuildingA, "awake"}, SetupOfficeEvent, {self})
  self:_CreateEvent(Event.ObjectProximity, {
    Player.GetAnyCharacter(),
    uBuildingA,
    "<",
    300
  }, MrxVoSequence.Start, {
    {
      {
        mattias = "Mattias-In-Mission-Contract-Pmc02-25",
        jennifer = "Jennifer-In-Mission-Contract-Pmc02-26",
        chris = "Chris-In-Mission-Contract-Pmc02-27"
      },
      0.5,
      "Fiona-In-Mission-Contract-Pmc02-28"
    }
  })
  oRigDeath = self:_CreateEvent(Event.ObjectDeath, {uOilRig}, Failure, {self, "rig"})
  MrxVoSequence.Start({
    "Fiona-Banter-Contract-Pmc02-01",
    0.5,
    {
      mattias = "mattias-Banter-Contract-Pmc02-02",
      jennifer = "jennifer-Banter-Contract-Pmc02-03",
      chris = "chris-Banter-Contract-Pmc02-04"
    },
    0.5,
    "Fiona-Banter-Contract-Pmc02-05",
    1,
    {
      mattias = "mattias-Banter-Contract-Pmc02-06",
      jennifer = "jennifer-Banter-Contract-Pmc02-07",
      chris = "chris-Banter-Contract-Pmc02-08"
    }
  })
end

function SetupOfficeEvent(self)
  oOfficeDeath = self:_CreateEvent(Event.ObjectHealth, {
    uBuildingA,
    "floor02.piece1b",
    "<",
    1
  }, Failure, {self, "rig"})
end

function Failure(self, sReason)
  if sReason == "rig" then
    oOfficeObjective:Cancel()
    self:Cleanup(true)
    self:_SetCancelMessage("[PmcCon002.objective.failed]")
    MrxVoSequence.Start({
      "Fiona-In-Mission-Contract-Pmc02-17",
      0.5,
      {
        mattias = "Mattias-In-Mission-Contract-Pmc02-19",
        jennifer = "Jennifer-In-Mission-Contract-Pmc02-20",
        chris = "Chris-In-Mission-Contract-Pmc02-21"
      },
      {
        self.Cancel,
        {self}
      }
    })
  end
end

function Cleanup(self, bContinue)
  if uMarker then
    Marker.Remove(uMarker)
    _G.Minimap:DeleteObjective("Enter office")
  end
  local uAlarm = Pg.GetGuidByName("oilrig_alarm")
  Vehicle.SetParts(uAlarm, "LightFront", true)
  Vehicle.SetParts(uAlarm, "CtrlRotation", true)
  Sound.StopSound(uAlarm, "fol_alarm_bldg_01")
  Sound.StopSound(uOilRig, "fol_alarm_bldg_01")
  Sound.StopSound(Pg.GetGuidByName("PmcCon002_OilrigD"), "fol_alarm_bldg_01")
  Sound.StopSound(Pg.GetGuidByName("PmcCon002_OilrigA"), "fol_alarm_bldg_01")
  Pg.RemoveContextAction(Pg.GetGuidByName("PMC002_Office"))
  if uBlanco then
    Human.SetAllowCorpseCleanup(uBlanco, false)
    Pg.RemoveContextAction(uBlanco)
    Object.Remove(uBlanco)
  end
  if not bContinue then
    MrxTaskContract.Cleanup(self)
  end
end

function PlayOfficeCinematic(self, uCharacter)
  self:Cleanup(true)
  Event.Delete(oOfficeDeath)
  
  local function _MoviePlayed()
    MrxState.Exit(MrxState.STATE_WAITFORGAME, self.SpawnBlanco, {self, uCharacter})
    Object.SetTransformToObject(uCharacter, Pg.GetGuidByName("PmcCon002 Exit"))
    Event.Create(Event.TimerRelative, {0.6}, ExplodeHero, {uCharacter})
  end
  
  local function _PlayMovie()
    local sHeroLetter = MrxUtil.GetCharacterIdentity(uCharacter)
    sHeroLetter = sHeroLetter and string.upper(string.sub(sHeroLetter, 1, 1))
    if sHeroLetter ~= "M" and sHeroLetter ~= "J" and sHeroLetter ~= "C" then
      sHeroLetter = "M"
    end
    Hud.Cinematic:Show({
      sMovie = "10_BRV_" .. sHeroLetter,
      fCallback = _MoviePlayed,
      bSubtitles = true
    })
  end
  
  MrxState.Enter(MrxState.STATE_WAITFORGAME, _PlayMovie)
end

function ExplodeHero(uCharacter)
  local x, y, z = Object.GetPosition(Pg.GetGuidByName("PmcCon002 Explosion"))
  Pg.Spawn("Explosion (Force)", x, y, z)
  Pg.Spawn("global_particle_explosion_c4", x, y, z)
  Event.Create(Event.TimerRelative, {0.01}, function()
    Human.Knockdown(uCharacter, 0.5)
  end)
end

function SpawnBlanco(self, uCharacter)
  local uAlarm = Pg.GetGuidByName("oilrig_alarm")
  Vehicle.SetParts(uAlarm, "LightFront", true)
  Vehicle.SetParts(uAlarm, "CtrlRotation", true)
  Sound.CueSound(uAlarm, "fol_alarm_bldg_01")
  Sound.CueSound(uOilRig, "fol_alarm_bldg_01")
  Sound.CueSound(Pg.GetGuidByName("PmcCon002_OilrigD"), "fol_alarm_bldg_01")
  Sound.CueSound(Pg.GetGuidByName("PmcCon002_OilrigA"), "fol_alarm_bldg_01")
  DangerousBuilding.TurnOn({
    Pg.GetGuidByName("PmcCon002_OilrigB"),
    Pg.GetGuidByName("PmcCon002_OilrigC"),
    Pg.GetGuidByName("PmcCon002_OilrigD")
  }, true)
  Pg.SpawnFromCamera("Alouette3 Transport (VZ) (Pursuit)", 300, 100)
  Pg.SpawnFromCamera("Alouette3 Transport (VZ) (Pursuit)", -300, 100)
  MrxLayerManager.Add({
    "VZ_state_PmcCon002_Blanco"
  }, RunBlancoRun, {self, uCharacter})
end

function RunBlancoRun(self, uCharacter)
  Event.Delete(oRigDeath)
  local uBlanco = Pg.GetGuidByName("PmcCon002 Blanco")
  oVerifyBlanco = self:CreateChild({
    sName = "Verify Blanco",
    sModuleName = "MrxTaskObjectiveVerify",
    vTgtInclude = {uBlanco},
    sDspShortDesc = "[PmcCon002.objective.verifyblanco]",
    sFactionId = "Gur",
    fOnTargetCaptured = function()
      self:_SetPlayer1Bonus(1000000)
      self:_SetPlayer2Bonus(1000000)
    end,
    fOnTargetSubdued = function()
      self:StopBlancoTaunt()
      MrxVoSequence.Start({
        {
          "Blanco-In-Mission-Contract-Pmc02-37",
          uBlanco
        }
      })
    end,
    tOnTargetDestroyed = {
      {
        StopBlancoTaunt,
        {self}
      }
    },
    tOnComplete = {
      {
        StartDestroyRig,
        {self}
      }
    }
  })
  oTauntTimer = self:_CreateEvent(Event.TimerRelative, {
    math.randi(5) + 3
  }, PlayBlancoTaunt, {self})
  self:_CreateEvent(Event.TimerRelative, {1.5}, OKVO, {self})
end

function OKVO(self)
  MrxVoSequence.Start({
    {
      mattias = "Fiona-In-Mission-Contract-Pmc02-09",
      jennifer = "Fiona-In-Mission-Contract-Pmc02-10",
      chris = "Fiona-In-Mission-Contract-Pmc02-11"
    },
    0.2,
    {
      mattias = "mattias-In-Mission-Contract-Pmc02-12",
      jennifer = "jennifer-In-Mission-Contract-Pmc02-13",
      chris = "chris-In-Mission-Contract-Pmc02-14"
    }
  })
end

function PlayBlancoTaunt(self)
  local uBlanco = Pg.GetGuidByName("PmcCon002 Blanco")
  local tCues = {
    "Blanco-In-Mission-Contract-Pmc02-32",
    "Blanco-In-Mission-Contract-Pmc02-33",
    "Blanco-In-Mission-Contract-Pmc02-34",
    "Blanco-In-Mission-Contract-Pmc02-35",
    "Blanco-In-Mission-Contract-Pmc02-36"
  }
  MrxVoSequence.Start({
    {
      MrxUtil.GetRandomTableElement(tCues),
      uBlanco
    }
  })
  oTauntTimer = self:_CreateEvent(Event.TimerRelative, {
    math.randi(8) + 5
  }, PlayBlancoTaunt, {self})
end

function StopBlancoTaunt(self)
  if oTauntTimer then
    Event.Delete(oTauntTimer)
  end
end

function StartDestroyRig(self)
  if not Object.IsAlive(Pg.GetGuidByName("PMC002 Oilrig")) then
    self:Complete()
  end
  self:_PlayVo(0, "Fiona-In-Mission-Contract-Pmc02-15")
  self.DestroyRig = self:CreateChild({
    sName = "Destroy oil rig",
    sModuleName = "MrxTaskObjectiveDestroy",
    vTgtInclude = {
      "PMC002 Oilrig"
    },
    sDspShortDesc = "[PmcCon002.objective.destroyoilrig]",
    fOnComplete = function()
      self:_CreateEvent(Event.TimerRelative, {24}, RigIsDead, {self})
    end,
    tOnCancel = {
      {
        self.Cancel,
        {self}
      }
    }
  })
end

function RigIsDead(self)
  MrxLayerManager.MarkForRemoval("vz_state_mer_oilrig_pristine")
  MrxVoSequence.Start({
    "Fiona-In-Mission-Contract-Pmc02-16",
    1,
    {
      self.Complete,
      {self}
    }
  })
end
