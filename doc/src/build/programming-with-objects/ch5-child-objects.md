## Chapter 5: Child Objects
In the previous chapter, we walked through various ways of wrapping an object in another object. There are a few limitations in object wrapping:
1. An wrapped object no longer exists independently on-chain. One cannot access it directly, not can view it directly in the explorer.
2. Objects can become very large if it wraps several objects inside. Larger objects can lead to higher gas fee in transactions. More importantly, there is an upper bound on object size.
3. As we will see in future chapters when we introduce the `Bag` library, there will be use cases where we need to store a vector of objects of heterogeneous types. Since the Move `vector` type must be templated on one single type `T`, there is no way to wrap them into a single vector.

Sui provides another way to represent object relationships: object can own obejcts. In the first chapter, we introduced libraries for tranferring objects to an account address. In this chapter, we will introduce libraries that allow you transfer objects to other objects.

### Create Child Objects
#### transfer_to_object
Assume we own two objects in our account address. To make one object own the other object, we can use the following API in the `Transfer` library:
```rust
public fun transfer_to_object<T: key, R: key>(
    obj: T,
    owner: &mut R,
): ChildRef<T>;
```
The first argument `obj` will become a child object of the second argument `owner`. `obj` must be passed by-value, i.e. it will be fully consumed, and cannot be accessed again within the same transaction (similar to `transfer` function). After calling this function, the owner of `obj` on-chain will change from the account address, to the object ID of the `owner` object.

The function returns a special struct `ChildRef<T>` where `T` matches the type of the child object. It represents a reference to the child object. Since `ChildRef` is a struct type without `drop` ability, Move ensures that the return value cannot be dropped. This ensures that the caller of the function must put the reference somewhere and cannot forget about it. This is very important because latter on if we attempt to delete the parent object, the existence of the child references force us to take care of them. Otherwise we may end up in a situation where we deleted the parent object, but there are still some child objects, and these child objects will be locked forever (as we will explain in latter sections). In the last section, we will also see how this reference is used to move around child objects and prevent making mistakes.

Let's look at some code. The full source code can be found in [ObjectOwner.move](https://github.com/MystenLabs/sui/tree/main/sui_core/src/unit_tests/data/object_owner/sources/ObjectOwner.move).

First we define two object types for the parent and the child:
```rust
struct Parent has key {
    id: VersionedID,
    child: Option<ChildRef<Child>>,
}

struct Child has key {
    id: VersionedID,
}
```
`Parent` type contains a `child` field that is an optional child reference to an object of `Child` type.
First we define an API to create an object of `Child` type:
```rust
public(script) fun create_child(ctx: &mut TxContext) {
    Transfer::transfer(
        Child { id: TxContext::new_id(ctx) },
        TxContext::sender(ctx),
    );
}
```
The above function creates a new object of `Child` type and transfer it to the sender account address of the transaction, i.e. after this call, the sender account owns the object.
Similarly, we can define an API to create an object of `Parent` type:
```rust
public(script) fun create_parent(ctx: &mut TxContext) {
    let parent = Parent {
        id: TxContext::new_id(ctx),
        child: Option::none(),
    };
    Transfer::transfer(parent, TxContext::sender(ctx));
}
```
Since the `child` field is `Option` type, we can start with `Option::none()`.
Now we can define an API that make an object of `Child` a child of an object of `Parent`:
```rust
public(script) fun add_child(parent: &mut Parent, child: Child, _ctx: &mut TxContext) {
    let child_ref = Transfer::transfer_to_object(child, parent);
    Option::fill(&mut parent.child, child_ref);
}
```
This function takes `child` by-value, calls `transfer_to_object` to transfer the `child` object to the `parent`, and returns a `child_ref`.
After that, we can fill the `child` field of `parent` with `child_ref`.
If we comment out the second line, Move compiler will complain that we cannot drop `child_ref`.
At the end of the `add_child` call, we have the following ownership relationship:
1. Sender account address owns a `Parent` object
2. The `Parent` object owns a `Child` object.

#### transfer_to_object_id
In the above example, `Parent` has an optional child field. What if the field is not optional? We will have to construct `Parent` with a `ChildRef`. However in order to have a `ChildRef`, we have to transfer the child object to the parent object first. This creates a paradox. We cannot create parent unless we have a `ChildRef`, and we cannot have a `ChildRef` unless we already have the parent object. To solve this exact problem and be able to construct a non-optional `ChildRef` field, we provide another API that allows you to transfer an object to object ID, instead of to object:
```rust
public fun transfer_to_object_id<T: key>(
    obj: T,
    owner_id: VersionedID,
): (VersionedID, ChildRef<T>);
```
To use this API, we don't need to create a parent object yet, but we only need the object ID of the parent object, which can be created in advance through `TxContext::new_id(ctx)`. The function returns a tuple: it will return the `owner_id` that was passed in, along with the `ChildRef` representing a reference to the child object `obj`. It may seem strange that we require passing in `owner_id` by-value only to return it. This is to ensure that the caller of the function does indeed own a `VersionedID` that hasn't been used in any object yet. Without this it can be easy to make mistakes.
Let's see how this is used in action. First we define another object type that has a non-optional child field:
```rust
struct AnotherParent has key {
    id: VersionedID,
    child: ChildRef<Child>,
}
```
And let's see how we define the API to create `AnotherParent` instance:
```rust
public(script) fun create_another_parent(child: Child, ctx: &mut TxContext) {
    let id = TxContext::new_id(ctx);
    let (id, child_ref) = Transfer::transfer_to_object_id(child, id);
    let parent = AnotherParent {
        id,
        child: child_ref,
    };
    Transfer::transfer(parent, TxContext::sender(ctx));
}
```
In the above function, we need to first create the ID of the new parent object. With the ID, we can then transfer the child object to it by calling `transfer_to_object_id`, obtaining a reference `child_ref`. With both `id` and `child_ref`, we can create an object of `AnotherParent`, which we would eventually transfer it to the sender's account.

### Use Child Objects
We have explained in the first chapter that, in order to use an owned object, the object owner must be the transaction sender. What about objects owned by objects? We require that the object's owner object must also be passed as an argument in the Move call. For example, if object A owns object B, and object B owns object C, to be able to use C when calling a Move entry function, one must also pass B in the argument; and since B is in the argument, A must also be in the argument. This essentially mean that to use an object, its entire ownership ancestor chain must be included, and the account owner of the root ancestor must match the sender of the transaction.

Let's look at how we could use the child object created earlier. Let's define two entry functions:
```rust
public(script) fun mutate_child(_child: &mut Child, _ctx: &mut TxContext) {}
public(script) fun mutate_child_with_parent(_child: &mut Child, _parent: &mut Parent, _ctx: &mut TxContext) {}
```
The first function requires only one object argument, which is an `Child` object. The second function requires two arguments, a `Child` object and a `Parent` object. Both functions are made empty since what we care about here is not the mutation logic, but whether you are able to make a call to them at all.
Both functions will compile successfully, because object ownership relationships are dynamic properties and the compiler cannot forsee.

Let's try to interact with these two entry functions on-chain and see what happens. First we publish the sample code:
```
$ wallet publish --path sui_core/src/unit_tests/data/object_owner --gas-budget 5000
```
```
----- Publish Results ----
The newly published package object ID: 0x3cfcee192b2fbafbce74a211e40eaf9e4cb746b9
```
Then we create a child object:
```
$ export PKG=0x3cfcee192b2fbafbce74a211e40eaf9e4cb746b9
$ wallet call --package $PKG --module ObjectOwner --function create_child  --gas-budget 1000
```
```
----- Transaction Effects ----
Created Objects:
  - ID: 0xb41d157fdeda968c5b5f0d8b87b6ebb84d7d1941 , Owner: Account Address ( 0x5f67488c28c46e56bcefb808ae499ef323c1236d )
```
At this point we only created the child object, but it's still owned by an account address. We can verify that we should be able to call `mutate_child` function by only passing in the child object:
```
$ export CHILD=0xb41d157fdeda968c5b5f0d8b87b6ebb84d7d1941
$ wallet call --package $PKG --module ObjectOwner  --function mutate_child --args $CHILD --gas-budget 1000
```
```
----- Transaction Effects ----
Status : Success
Mutated Objects:
  - ID: 0xb41d157fdeda968c5b5f0d8b87b6ebb84d7d1941
```
Indeed the transasaction succeeded.

Now let's create the `Parent` object as well:
```
$ wallet call --package $PKG --module ObjectOwner --function create_parent --gas-budget 1000
```
```
----- Transaction Effects ----
Created Objects:
  - ID: 0x2f893c18241cfbcd390875f6e1566f4db949392e
```
Now we can make the parent object own the child object:
```
$ export PARENT=0x2f893c18241cfbcd390875f6e1566f4db949392e
$ wallet call --package $PKG --module ObjectOwner --function add_child --args $PARENT $CHILD --gas-budget 1000
```
```
----- Transaction Effects ----
Mutated Objects:
- ID: 0xb41d157fdeda968c5b5f0d8b87b6ebb84d7d1941 , Owner: Object ID: ( 0x2f893c18241cfbcd390875f6e1566f4db949392e )
```
As we can see, the owner of the child object has been changed to the parent object ID.

Now if we try to call `mutate_child` again, we will see an error:
```
$ wallet call --package $PKG --module ObjectOwner  --function mutate_child --args $CHILD --gas-budget 1000
```
```
Object 0xb41d157fdeda968c5b5f0d8b87b6ebb84d7d1941 is owned by object 0x2f893c18241cfbcd390875f6e1566f4db949392e, which is not in the input
```

To be able to mutate the child object, we must also pass the parent object as argument. Hence we need to call the `mutate_child_with_parent` function:
```
$ wallet call --package $PKG --module ObjectOwner  --function mutate_child_with_parent --args $CHILD $PARENT --gas-budget 1000
```
It will finish successfully.

### Transfer Child Objects
In this section, we will introduce a few more APIs that will allow us safely move around child objects.

There are two ways to transfer a child object:
1. Transfer it to an account address, thus it will no longer be a child object after the transfer.
2. Transfer it to another object, thus it will still be a child object but the parent object changed.

#### transfer_child_to_address
First of all, let's look at how to transfer a child object to an account address. The [Transfer](https://github.com/MystenLabs/sui/blob/main/sui_programmability/framework/sources/Transfer.move) library provides the following API:
```rust
public fun transfer_child_to_address<T: key>(
    child: T,
    child_ref: ChildRef<T>,
    recipient: address,
);
```
`transfer_child_to_address` transfers a currently child object to an account address. This function takes 3 arguments: `child` is the child object we wish to transfer, `child_ref` is the reference to it that was obtained when we previously transferred it to its current parent, and `recipient` is the recipient account address. After the transfer, the `recipient` account address now owns this object.
There are two important things worth mentioning:
1. Requiring `child_ref` as an argument ensures that the old parent won't have an out-of-dated reference to the child object, and this reference is properly destroyed by the library during the transfer.
2. This function has no return value. We no longer need a `ChildRef` because the object is no longer a child object.

To demonstrate how to use this API, let's implement a function that removes a child object from a parent object and transfer it back to the account owner:
```rust
public(script) fun remove_child(parent: &mut Parent, child: Child, ctx: &mut TxContext) {
    let child_ref = Option::extract(&mut parent.child);
    Transfer::transfer_child_to_address(child, child_ref, TxContext::sender(ctx));
}
```
In the above function, the reference to the child is extracted from the `parent` object, which is then passed together with the `child` object to the `transfer_child_to_address`, with recipient as the sender of the transaction. It is important to note that this function must also take the `child` object as an argument. Move is not able to obtain the child object only from the reference. An object must always be explicitly provided in the transaction to make the transfer work. As we explaiend earlier, the fact that `transfer_child_to_address` requires the child reference as an argument helps guarantees that the `parent` object no longer holds a reference to the child object.

#### transfer_child_to_object
Another way to transfer a child object is to transfer it to another parent. The API is also defined in the Transfer library:
```rust
public fun transfer_child_to_object<T: key, R: key>(
    child: T,
    child_ref: ChildRef<T>,
    owner: &mut R,
): ChildRef<T>;
```
After this call, the object `child` will become a child object of the object `owner`.
Comparing to the previous API, there are two primary differences:
1. Instead of transferring the object to an recipient address, here we need to provide a mutable reference to the new parent objecet `owner`. Although we are not mutating the new parent objeect in practice, we require `mut` to make sure the new owner is not an immutable object: immutable objects cannot have child objects.
2. The function returns a new `ChildRef`. This is because we are transferring this object to a new parent, and hence a new reference is created to represent this child relationship, which will be different from the old child reference.

To see how to use this API, let's define a function that could transfer a child object to a new parent:
```rust
public(script) fun transfer_child(parent: &mut Parent, child: Child, new_parent: &mut Parent, _ctx: &mut TxContext) {
    let child_ref = Option::extract(&mut parent.child);
    let new_child_ref = Transfer::transfer_child_to_object(child, child_ref, new_parent);
    Option::fill(&mut new_parent.child, new_child_ref);
}
```
Similar to `remove_child`, the `child` object msut be passed explicitly by-value in the arguments. First of all we extract the existing child reference, and pass it to `transfer_child_to_object` along with `child`, and a mutable reference to `new_parent`. This call will return a new child reference. We then fill the `new_parent`'s `child` field with this new reference. Since `ChildRef` type is not droppable, `Option::fill` will fail if `new_parent.child` already contains an existing `ChildRef`. This ensures that we never accidentally drops a `ChildRef` without properly transferring the child.

### Delete Child Objects
For the same reasons that transferring child objects require both the child object and the reference, deleting child objects directly without taking care of the child reference will lead to a stale reference pointing to a non-existing object after the deletion.
In order to delete a child object, we must first transfer this child object to an account address, which makes this object a regular account-owned object instead of a child object, and hence can be deleted normally.

What happens if we try to delete a child directly using what we learned in the first chapter, without taking the child reference? Let's find out. We can define a simple `delete_child` method like this:
```rust
public(script) fun delete_child(child: Child, _parent: &mut Parent, _ctx: &mut TxContext) {
    let Child { id } = child;
    ID::delete(id);
}
```
If you follow the wallet interaction above and then try to call the `delete_child` function here on a child object, you will see the following error:
```
An object that's owned by another object cannot be deleted or wrapped.
It must be transferred to an account address first before deletion
```
If we follow the suggestion, fist call `remove_child` to turn this child object to an account-owned object, and then call `delete_child` again, it will succeed!
