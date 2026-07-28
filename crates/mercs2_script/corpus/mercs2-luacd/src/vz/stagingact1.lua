function Start()
  Debug.Printf("***************Staging Act One is GO GO GO")
end

function GurBaseGatePatrol()
  Event.Create(Event.ObjectHibernation, {
    Pg.GetGuidByName("Patrol_GurBase_Gate"),
    "awake"
  }, StartPatrol, {
    Pg.GetGuidByName("Patrol_GurBase_Gate"),
    Pg.GetGuidByName("path_gate_patrol"),
    "loop",
    "lowpri",
    ".4"
  })
end

function GurBaseTrailerPatrol()
  Event.Create(Event.ObjectHibernation, {
    Pg.GetGuidByName("Guerilla_Trailer_Patrol"),
    "awake"
  }, StartPatrol, {
    Pg.GetGuidByName("Guerilla_Trailer_Patrol"),
    Pg.GetGuidByName("Path_Trailer_Loop"),
    "loop",
    "lowpri",
    ".1"
  })
end

function GurBaseRoadPatrolOne()
  Debug.Printf("***************Gur Base Road One Patrol GO GO GO")
  Event.Create(Event.ObjectHibernation, {
    Pg.GetGuidByName("Guerilla_Patrol_RoadOne"),
    "awake"
  }, StartPatrol, {
    Pg.GetGuidByName("Guerilla_Patrol_RoadOne"),
    Pg.GetGuidByName("Path_GurBase_RoadOne"),
    "loop",
    "lowpri",
    ".5"
  })
end

function GurBaseEarthmoverPatrol()
  Event.Create(Event.ObjectHibernation, {
    Pg.GetGuidByName("\t"),
    "awake"
  }, StartPatrol, {
    Pg.GetGuidByName("Guerilla_Earthmover_Patrol"),
    Pg.GetGuidByName("Path_EarthMover_Patrol"),
    "loop",
    "lowpri",
    ".5"
  })
end

function GurBaseMoverarmPatrol()
  Event.Create(Event.ObjectHibernation, {
    Pg.GetGuidByName("Guerilla_Moverarm_Patrol"),
    "awake"
  }, StartPatrol, {
    Pg.GetGuidByName("Guerilla_Moverarm_Patrol"),
    Pg.GetGuidByName("Path_GurBase_MoverArm"),
    "loop",
    "lowpri",
    ".2"
  })
end

function GurBaseSquadPatrolOne()
  Event.Create(Event.ObjectHibernation, {
    Pg.GetGuidByName("Squad_PatrolOne"),
    "awake"
  }, StartPatrol, {
    Pg.GetGuidByName("Squad_PatrolOne"),
    Pg.GetGuidByName("Path_Squad_PatrolOne"),
    "loop",
    "lowpri",
    ".2"
  })
end

function GurBaseRoadPatrolTwo()
  Event.Create(Event.ObjectHibernation, {
    Pg.GetGuidByName("Guerilla_Patrol_RoadTwo"),
    "awake"
  }, StartPatrol, {
    Pg.GetGuidByName("Guerilla_Patrol_RoadTwo"),
    Pg.GetGuidByName("Path_GurBase_RoadTwo"),
    "loop",
    "lowpri",
    ".2"
  })
end

function GurBaseFrontPatrolOne()
  Event.Create(Event.ObjectHibernation, {
    Pg.GetGuidByName("Guerilla_Patrol_FrontOne"),
    "awake"
  }, StartPatrol, {
    Pg.GetGuidByName("Guerilla_Patrol_FrontOne"),
    Pg.GetGuidByName("Path_GurBase_Front_PatrolOne"),
    "loop",
    "lowpri",
    ".2"
  })
end

function StartPatrol(uActor, uTarget, mode, priority, haste)
  Debug.Printf("**************************************** Gur Base Patrol StartPatrol is active")
  if priority == nil then
    priority = "lowpri"
  end
  tGoalParams = {
    AIGuid = uActor,
    Goal = "PathMove",
    Target = uTarget,
    Start = "First",
    Priority = priority,
    Mode = mode,
    Haste = haste
  }
  Event.Create(Event.TimerRelative, {1}, Ai.Goal, {tGoalParams})
end
