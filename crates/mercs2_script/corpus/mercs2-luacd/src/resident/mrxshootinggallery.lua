import("MrxSubtitle")
import("MrxVoSequence")

function RemoveWeapons(uPlayer)
  tWeapons = {}
  uPrimaryWeapon = Human.Inventory.GetPrimaryWeapon(uPlayer)
  Debug.Printf("Deequipping primary weapon " .. tostring(uPrimaryWeapon))
  if uPrimaryWeapon then
    Human.Inventory.DropWeapon(uPlayer, uPrimaryWeapon)
    x, y, z = Object.GetPosition(uPlayer)
    Object.DisablePhysics(uPrimaryWeapon)
    Object.SetPosition(uPrimaryWeapon, x, y - 5, z)
    Debug.Printf("dropping weapon " .. tostring(uPrimaryWeapon))
    tWeapons.Primary1 = uPrimaryWeapon
  end
  uPrimaryWeapon2 = Human.Inventory.GetPrimaryWeapon(uPlayer)
  if uPrimaryWeapon2 then
    Human.Inventory.DropWeapon(uPlayer, uPrimaryWeapon2)
    x, y, z = Object.GetPosition(uPlayer)
    Object.DisablePhysics(uPrimaryWeapon2)
    Object.SetPosition(uPrimaryWeapon2, x, y - 5, z)
    tWeapons.Primary2 = uPrimaryWeapon2
  end
  uSecondaryWeapon = Human.Inventory.GetSecondaryWeapon(uPlayer)
  if uSecondaryWeapon then
    Human.Inventory.DropWeapon(uPlayer, uSecondaryWeapon)
    x, y, z = Object.GetPosition(uPlayer)
    Object.DisablePhysics(uSecondaryWeapon)
    Object.SetPosition(uSecondaryWeapon, x, y - 5, z)
    tWeapons.Secondary1 = uSecondaryWeapon
  end
  uSecondaryWeapon2 = Human.Inventory.GetSecondaryWeapon(uPlayer)
  if uSecondaryWeapon2 then
    Human.Inventory.DropWeapon(uPlayer, uSecondaryWeapon2)
    x, y, z = Object.GetPosition(uPlayer)
    Object.DisablePhysics(uSecondaryWeapon2)
    Object.SetPosition(uSecondaryWeapon2, x, y - 5, z)
    tWeapons.Secondary2 = uSecondaryWeapon2
  end
  Debug.Printf(tostring(tWeapons.Primary1))
  Debug.Printf(tostring(tWeapons.Primary2))
  Debug.Printf(tostring(tWeapons.Secondary1))
  Debug.Printf(tostring(tWeapons.Secondary2))
  return tWeapons
end

function ReturnWeapons(uPlayer, tWeapons)
  Debug.Printf("Returning weapons now!")
  Debug.Printf(tostring(tWeapons.Primary1))
  Debug.Printf(tostring(tWeapons.Primary2))
  Debug.Printf(tostring(tWeapons.Secondary1))
  Debug.Printf(tostring(tWeapons.Secondary2))
  Object.SetInfiniteAmmo(uPlayer, false)
  Debug.Printf("Turned off infinite ammo")
  uGalleryWeapon = Human.Inventory.GetPrimaryWeapon(uPlayer)
  if uGalleryWeapon then
    Human.Inventory.DropWeapon(uPlayer, uGalleryWeapon)
    Debug.Printf("Dropped gallery weapon")
    x, y, z = Object.GetPosition(uPlayer)
    Object.DisablePhysics(uGalleryWeapon)
    Object.SetPosition(uGalleryWeapon, x, y - 5, z)
  end
  if tWeapons.Secondary1 then
    Human.Inventory.EquipWeapon(uPlayer, tWeapons.Secondary1)
    Debug.Printf("Trying to equip secondary weapon")
  end
  if tWeapons.Secondary2 then
    Human.Inventory.EquipWeapon(uPlayer, tWeapons.Secondary2)
  end
  if tWeapons.Primary2 then
    Human.Inventory.EquipWeapon(uPlayer, tWeapons.Primary2)
  end
  Debug.Printf("Returned Primary Weapon2!")
  if tWeapons.Primary1 then
    Human.Inventory.EquipWeapon(uPlayer, tWeapons.Primary1)
  end
  Debug.Printf("Returned Primary Weapon!")
end

local _evPlayerJoined

function ClearEvents()
  Event.Delete(_evNetSafeSetupBorder)
  Event.Delete(_BorderEventP1)
  Event.Delete(_BorderEventP2)
  if uFireLockVO then
    Event.Delete(uFireLockVO)
  end
  if _evPlayerJoined then
    Event.Delete(_evPlayerJoined)
    _evPlayerJoined = nil
  end
end

function Reset()
  local uChar1 = Player.GetPrimaryCharacter()
  local uChar2 = Player.GetSecondaryCharacter()
  if uChar1 then
    Human.SetFireLock(uChar1, false)
  end
  if uChar2 then
    Human.SetFireLock(uChar2, false)
  end
  ClearEvents()
end

function NetSafeSetupBorder(uBorderName)
  if Net.IsClient() then
    SetupBorder(uBorderName)
    _evNetSafeSetupBorder = Event.Create(Event.GameStateChange, {
      "WaitForTether",
      "exit"
    }, SetupBorder, {uBorderName})
  end
end

function SetupClientBorder(uBorderName)
  local uChar2 = Player.GetSecondaryCharacter()
  if uChar2 then
    Event.Create(Event.ObjectHibernation, {uChar2, "awake"}, function()
      _BorderEventP2 = Event.Create(Event.Boundary, {
        uChar2,
        uBorderName,
        "exit"
      }, SteppedOut, {uChar2, uBorderName})
    end)
  end
end

function SetupBorder(uBorderName)
  Debug.Printf("inside mrxshootinggallery.setupborder")
  if Net.IsServer() then
    Net.SetShootingGalleryBorder(uBorderName)
  end
  if not uBorderName then
    Reset()
  else
    Debug.Printf("SetupBorder(" .. tostring(uBorderName) .. ")")
    Reset()
    _evPlayerJoined = Event.CreatePersistent(Event.ScriptEvent, {
      "mpPlayerJoin",
      function(tData)
        return Net.IsServer() and not Player.IsLocal(tData[1])
      end
    }, SetupClientBorder, {uBorderName})
    local uChar1 = Player.GetPrimaryCharacter()
    local uChar2 = Player.GetSecondaryCharacter()
    if uChar1 then
      _BorderEventP1 = Event.Create(Event.Boundary, {
        uChar1,
        uBorderName,
        "exit"
      }, SteppedOut, {uChar1, uBorderName})
    end
    if uChar2 then
      _BorderEventP2 = Event.Create(Event.Boundary, {
        uChar2,
        uBorderName,
        "exit"
      }, SteppedOut, {uChar2, uBorderName})
    end
    Debug.Printf("set up event")
  end
end

function SteppedOut(uChar, uBorderName)
  Debug.Printf("SteppedOut(" .. tostring(uChar) .. ", " .. tostring(uBorderName) .. ")")
  if Player.GetLocalCharacter() == uChar then
  end
  Human.SetFireLock(uChar, true)
  uPrimaryWeapon = Human.Inventory.GetPrimaryWeapon(Player.GetLocalCharacter())
  uFireLockVO = Event.Create(Event.WeaponEvent, {
    Player.GetLocalCharacter(),
    "FireLock",
    uPrimaryWeapon
  }, MrxVoSequence.Start, {
    {
      "Fiona-In-Mission-MinorContract-Pmc31-08"
    }
  })
  if uChar == Player.GetPrimaryCharacter() then
    Event.Delete(_BorderEventP1)
    _BorderEventP1 = Event.Create(Event.Boundary, {
      uChar,
      uBorderName,
      "enter"
    }, SteppedIn, {uChar, uBorderName})
  elseif uChar == Player.GetSecondaryCharacter() then
    Event.Delete(_BorderEventP2)
    _BorderEventP2 = Event.Create(Event.Boundary, {
      uChar,
      uBorderName,
      "enter"
    }, SteppedIn, {uChar, uBorderName})
  end
end

function SteppedIn(uChar, uBorderName)
  Debug.Printf("SteppedIn(" .. tostring(uChar) .. ", " .. tostring(uBorderName) .. ")")
  Human.SetFireLock(uChar, false)
  if uChar == Player.GetPrimaryCharacter() then
    Event.Delete(_BorderEventP1)
    _BorderEventP1 = Event.Create(Event.Boundary, {
      uChar,
      uBorderName,
      "exit"
    }, SteppedOut, {uChar, uBorderName})
  elseif uChar == Player.GetSecondaryCharacter() then
    Event.Delete(_BorderEventP2)
    _BorderEventP2 = Event.Create(Event.Boundary, {
      uChar,
      uBorderName,
      "exit"
    }, SteppedOut, {uChar, uBorderName})
  end
end
